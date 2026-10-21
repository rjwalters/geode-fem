"""Reconcile frozen setup/anchor/calibration evidence before scientific selection.

Pure computations from reviewed producer source are replayed without importing
its optimizer/runtime launcher. This never calibrates, solves FEM or selects a
candidate. The caller must first verify the complete terminal capture.
"""
import ast
import hashlib
import json
import math
from pathlib import Path

from capture_archive import Reader
from design_evidence import RegisteredDesigns, bits, _bound_json
from selection import _exact
from validation_evidence import geometry, quality, same, number, verify_mesh
import producer_source


def _equal(a,b,label):
    if _exact(a) != _exact(b):raise ValueError('inconsistent '+label)


def _functions(repo, commit, name, names, *, globals=None):
    raw=producer_source.git(repo,'show',commit+':'+producer_source.HARNESS+name)
    tree=ast.parse(raw);nodes=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in names]
    if {n.name for n in nodes} != set(names):raise ValueError('missing reviewed setup computation')
    env={} if globals is None else dict(globals)
    exec(compile(ast.Module(body=nodes,type_ignores=[]),name,'exec'),env)
    return env


def _source_hashes(observed, expected, label):
    if observed != expected:raise ValueError(label+' changed reviewed sources')


#: The registered #1218 ablation block of an FD-ablation session.json.
ABLATION={'issue':1218,'methods':['slsqp','slsqp_fd'],'arms':{'slsqp':'A_prime','slsqp_fd':'F'},'post_hoc':True}
L0_MESH='benchmarks/motor_reference/geometry/meshes/icem_initial_sector_L0.msh'


def check(records, *, frozen, inputs, source, git_repo, reviewed_commit, cohort='main'):
    """Recompute setup records after capture and input byte verification.

    `inputs` are local paths for the already authenticated frozen artifacts.
    This function is also exercised on real live setup records as a schema probe;
    such a probe supplies no closed-study or optimizer eligibility evidence.
    cohort='fd_ablation' checks a #1218 `session.py --fd-ablation` setup: the
    main session's setup with no L0 model or L0 calibration. Its frozen-start
    preflight still moves the committed L0 mesh, bound by the reviewed sources.
    """
    if cohort not in ('main','fd_ablation'):raise ValueError('unknown setup cohort')
    session=records['session.json'];bindings=records['bindings.json'];files=source['files']
    _equal(session,frozen['session'],'frozen session')
    if cohort=='fd_ablation' and (_exact(session.get('ablation'))!=_exact(ABLATION) or set(session.get('models',{}))!={'L1'}
            or 'training_L0' in frozen['inputs'] or 'calibration_secondary.json' in bindings.get('files',{})):
        raise ValueError('changed registered FD-ablation session')
    if (session.get('label')!=cohort or session.get('budget_fe')!=120. or session.get('checkpoint_fe')!=60.
            or type(session.get('procs')) is not int or session['procs']<1
            or session['unit_seconds']<=0 or session.get('primary_model')!='L1'):
        raise ValueError('changed registered main session')
    _equal(bindings['procs'],session['procs'],'worker count')
    _equal(bindings['unit_seconds'],session['unit_seconds'],'common unit')
    if bindings.get('dispatch_settings',{}).get('gate') is not True:
        raise ValueError('main dispatch did not use host gate')
    setup=_functions(git_repo,reviewed_commit,'recovery_bindings.py',{'validate_setup'})
    setup['validate_setup'](frozen,bindings['provenance'],bindings['meshes'])
    reference_hash=files['benchmarks/motor_reference/reference_data.json']
    starts_hash=files['benchmarks/motor_optimization/harness/starts.json']
    definitions=RegisteredDesigns.load(inputs['reference_data'],inputs['starts'],reference_sha256=reference_hash,starts_sha256=starts_hash)
    starts=_bound_json(inputs['starts'],starts_hash)
    anchor_hash=files['benchmarks/motor_optimization/anchor.json']
    canonical=_bound_json(inputs['anchor'],anchor_hash)
    anchor=canonical['anchor']
    if canonical['provenance']['reference_data_sha256']!=reference_hash:
        raise ValueError('canonical anchor changed physical reference')
    for name,fraction in (('T50',.5),('T90',.1),('T99',.01)):
        same(anchor['targets'][name],fraction*anchor['F0'],'canonical target')
    same(anchor['mean_floor'],1.0025*anchor['mean0_full_current'],'canonical mean floor')
    if anchor['level']!=2 or frozen['inputs']['validation_mesh']['sha256']!=anchor['msh_sha256']:
        raise ValueError('canonical anchor and validator mesh differ')
    declared_provenance={'source_commit':reviewed_commit,'binary_sha256':frozen['inputs']['binary']['sha256'],
                        'source_files_sha256':hashlib.sha256(json.dumps(files,sort_keys=True,separators=(',',':')).encode()).hexdigest()}
    # Replay the reviewed host-suitability comparison. Its new values are not
    # substituted for the canonical target anchor.
    ap=_functions(git_repo,reviewed_commit,'anchor_preflight.py',{'finite','signature','compare','calibration_matches'},
                  globals={'math':math,'to_bits':bits,'TOLERANCE':1e-9})
    report=records['anchor_preflight.json']
    _equal(session.get('anchor_recomputation'),
           {'file':bindings['session_directory']+'/anchor_preflight.json',
            'sha256':bindings['files']['anchor_preflight.json']},'session anchor receipt')
    if (report.get('status')!='PASS' or report.get('spaces_agree_bitwise') is not True
            or report.get('relative_tolerance')!=1e-9 or report.get('absolute_torque_tolerance_Nm')!=1e-9
            or report.get('counted_in_run_budgets') is not False or report.get('counted_in_calibration_unit') is not False
            or report.get('tree_clean') is not True
            or report.get('canonical_anchor_sha256')!=anchor_hash or report.get('base_commit')!=reviewed_commit
            or report.get('binary_sha256')!=declared_provenance['binary_sha256']):
        raise ValueError('invalid anchor preflight binding/gate')
    expected_sources={n:h for n,h in files.items() if n.startswith(producer_source.HARNESS)
                      and Path(n).suffix=='.py' and str(Path(n).parent)+'/'==producer_source.HARNESS}
    _source_hashes(report['source_sha256'],expected_sources,'anchor preflight')
    expected_inputs={frozen['inputs'][n]['path']:frozen['inputs'][n]['sha256']
                     for n in ('anchor','training_L1','validation_mesh')}
    expected_inputs[frozen['source']['repo']+'/benchmarks/motor_reference/reference_data.json']=reference_hash
    _equal(report['input_sha256'],expected_inputs,'anchor preflight inputs')
    if [e.get('space') for e in report['sessions']]!=['a','b']:raise ValueError('missing or repeated anchor space')
    refs={};effective=[];signatures=[]
    for entry in report['sessions']:
        space=entry['space'];ready=entry['ready'];reply=entry['reply'];v=reply['validation']
        if (entry.get('status')!='PASS' or reply.get('ok') is not True or reply.get('registered') is not True
                or ready.get('session',{}).get('registered') is not True):
            raise ValueError('failed anchor session')
        compared=ap['compare'](v,canonical,space)
        if compared['status']!='PASS':raise ValueError('canonical anchor recomputation failed')
        _equal(entry['comparison'],compared,'anchor comparison')
        verify_mesh(v,anchor['msh_sha256'])
        for current in v['currents']:
            torques=current['torques'];mean=math.fsum(torques)/len(torques)
            std=math.sqrt(math.fsum((t-mean)**2 for t in torques)/len(torques))
            for name,value in (('mean',mean),('std',std)):
                if not math.isclose(number(current[name],name),value,rel_tol=1e-10,abs_tol=1e-14):
                    raise ValueError('anchor aggregates disagree with waveform')
        same(v['F'],sum(c['std']**2 for c in v['currents']),'anchor objective')
        for name in ('F0','mean0_full_current'):
            actual=number(ready['anchor'][name],name)
            if abs(actual-anchor[name])>4*2.220446049250313e-16*abs(anchor[name]):
                raise ValueError('anchor parse differs from registered four-epsilon rule')
        effective.append({k:ready['anchor'][k] for k in ('F0','mean0_full_current','level','msh_sha256')})
        refs[space]=definitions.adapt({'space':space,'evaluator_ready':ready,'anchor':ready['anchor']},
                                     {'x':starts['starts']['theta0'][space]},provenance=declared_provenance)
        signatures.append(ap['signature'](v))
    _equal(effective[0],effective[1],'effective per-space anchor')
    _equal(signatures[0],signatures[1],'same-host anchor waveforms')
    first=report['sessions'][0]['reply']['validation']
    _equal(report['calibration_reference'],{'F':first['F'],'mean_full':first['full_current_mean']},'calibration reference')
    # All twenty frozen starting-design checks must retain exact coordinates,
    # geometry and moved-mesh quality, with no FEM cost or replacement.
    pre=records['start_preflight.json']
    if (pre.get('status')!='PASS' or pre.get('fem_solves')!=0 or pre.get('counted_in_run_budgets') is not False
            or pre.get('starts_sha256')!=starts_hash or pre.get('binary_sha256')!=declared_provenance['binary_sha256']
            or pre.get('base_commit')!=reviewed_commit):raise ValueError('invalid frozen-start preflight')
    _source_hashes(pre['source_sha256'],{n:files[producer_source.HARNESS+n] for n in
                   ('start_preflight.py','make_starts.py','evaluator.py','run.py','resource_usage.py')},'start preflight')
    if cohort=='main':
        _equal(pre['mesh_sha256'],{str(i):frozen['inputs']['training_L'+str(i)]['sha256'] for i in (0,1)},'start preflight meshes')
    else:
        _equal(pre['mesh_sha256'],{'0':files[L0_MESH],'1':frozen['inputs']['training_L1']['sha256']},'start preflight meshes')
    if [(e.get('space'),e.get('level')) for e in pre['sessions']]!=[('a',0),('a',1),('b',0),('b',1)]:
        raise ValueError('incomplete start preflight matrix')
    geometry_definitions={}
    for entry in pre['sessions']:
        sp=entry['space'];level=entry['level'];ready=entry['ready']
        if entry.get('status')!='PASS' or ready['train_msh_sha256']!=pre['mesh_sha256'][str(level)]:
            raise ValueError('failed or mismatched start preflight session')
        _equal(ready['space'],starts['spaces'][sp],'starting space')
        if [c['start'] for c in entry['checks']]!=['theta0','lhs1','lhs2','lhs3','lhs4']:
            raise ValueError('frozen starts missing or replaced')
        for check in entry['checks']:
            reply=check['reply'];detail=reply['geometry'];x=starts['starts'][check['start']][sp]
            if (check.get('status')!='PASS' or check.get('reason') is not None or reply.get('ok') is not True
                    or reply.get('geometry_ok') is not True or reply.get('trigger') is not None
                    or check['x_bits']!=bits(x) or reply['x_bits']!=bits(x) or bits(reply['x'])!=bits(x)
                    or not quality(reply['quality'])):raise ValueError('starting design/geometry/moved mesh failed')
            defs={'guard_specs':{g['name']:{k:g[k] for k in ('scale','strict','unit')} for g in detail['guards']},
                  'discrete_names':[g['name'] for g in detail['discrete']]}
            if sp in geometry_definitions:_equal(defs,geometry_definitions[sp],'frozen geometry definitions')
            else:geometry_definitions[sp]=defs
            geometry({'geometry':detail,'admissible':detail['credit_admissible'],
                      'h_nonnegative':detail['admissible'],'min_h':detail['min_h']},**defs)
            if detail['admissible'] is not True or not reply.get('charges') or any(
                    type(c.get(k)) is not int or c[k]!=0 for c in reply['charges'] for k in ('forward_solves','adjoint_solves')):
                raise ValueError('starting-design feasibility or no-FEM rule failed')
    combine=_functions(git_repo,reviewed_commit,'session.py',{'median','combine_calibrations'})['combine_calibrations']
    calibrations=(('L1','calibration_primary.json'),('L0','calibration_secondary.json'))
    for label,key in calibrations if cohort=='main' else calibrations[:1]:
        envelope=records[key];cal=envelope['calibration'];host=cal['host']
        if (envelope.get('accepted_for_dispatch') is not True or host.get('gate') is not True
                or host.get('host_contaminated') is not False or host.get('monitor_error') is not None
                or cal.get('deterministic') is not True or cal.get('space_b_theta0_train_agrees_bitwise') is not True
                or not ap['calibration_matches'](cal,report) or cal['processes']!=session['procs']):
            raise ValueError('rejected calibration or changed host/reference gate')
        if (number(host.get('pre_run_load'),'pre-calibration load',minimum=0)>session['procs']+1
                or number(host.get('longest_overload_s'),'calibration overload',minimum=0)>60.):
            raise ValueError('calibration host thresholds failed')
        for row in cal['per_process']:
            if row.get('ok') is not True or row.get('repeats')!=3:raise ValueError('invalid calibration process')
            for field in ('validation_seconds','train_forward_seconds','train_adjoint_seconds'):
                if len(row[field])!=3 or any(number(x,field,minimum=0)<=0 for x in row[field]):
                    raise ValueError('incomplete cold calibration timings')
        recomputed=combine(cal['per_process'],common_unit=session['unit_seconds'] if label=='L0' else None)
        for key,value in recomputed.items():_equal(cal[key],value,'calibration '+key)
        _equal(cal['unit_seconds'],session['unit_seconds'],'reused common unit')
        model=session['models'][label]
        for field in ('weights','theta0_train'):_equal(model[field],cal[field],'training model '+field)
        if (model['train_level']!=int(label[1])
                or model['train_mesh']!=frozen['inputs']['training_'+label]['path']
                or model['train_mesh_sha256']!=frozen['inputs']['training_'+label]['sha256']):
            raise ValueError('calibrated training mesh changed')
    for key in ('weights','theta0_train'):_equal(session[key],session['models']['L1'][key],'primary model '+key)
    return {'effective_anchor':effective[0],'canonical_anchor':anchor,'definitions':definitions,
            'theta0':refs,'geometry_definitions':geometry_definitions,'provenance':declared_provenance,
            'unit_seconds':session['unit_seconds'],'budget_fe':session['budget_fe'],
            'can_select':False,'can_launch':False,
            'scope':'Checked original setup, anchor parsing and calibration; per-validation scientific adaptation and cohort selection remain required.'}


def verify(capture, *, git_repo):
    reader=Reader(capture['root'],capture['manifest_sha256'])
    root=reader.manifest['source_directory']
    names=('bindings.json','session.json','start_preflight.json','anchor_preflight.json',
           'calibration_primary.json','calibration_secondary.json')
    records={n:reader.json(root+'/'+n) for n in names}
    result=check(records,frozen=capture['frozen'],inputs=capture['input_paths'],
                 source=capture['source_verification'],git_repo=git_repo,
                 reviewed_commit=reader.manifest['reviewed_commit'])
    reader.stable()
    return result
