"""Read-only recovery-chain evidence, using the harness's pinned policy.

Never constructs Registry or executes a recovery. Copied directories retain
original source identities; archived external-evidence paths are mapped in memory.
This establishes attempt dispositions, not complete scientific/design provenance.
"""
from contextlib import ExitStack
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from trace_archive import Archive, _events, _json, _sha, load_generation

SNAPSHOT_KEYS = ('method','space','start','repeat','unit_seconds','budget_fe','session',
    'optimizer_completed','stop_reason','ledger','validations','attainment','checkpoint_60',
    'diagnostic_state','anchor','training_model')
OUTCOME_KEYS = ('optimizer_completed','stop_reason','diagnostic_status','ledger','interruption',
    'optimizer_cap_hit','surrogate_subproblem_cap_count','solver_calls','in_run_wall_seconds',
    'post_run_wall_seconds','resources','optimizer_snapshot')


def _directory(parent, name):
    if type(name) is not str or not name or Path(name).name != name or name in ('.','..'):
        raise ValueError('recovery directory escapes its parent')
    path=parent/name
    if path.is_symlink() or not path.is_dir():
        raise ValueError('missing or redirected recovery directory')
    return path


def _registry_events(raw):
    if not raw or not raw.endswith(b'\n'):
        raise ValueError('empty or torn registry journal')
    events=[_json(line) for line in raw.splitlines()]
    for index,event in enumerate(events):
        if (type(event) is not dict or type(event.get('sequence')) is not int
                or event['sequence']!=index):
            raise ValueError('registry sequence is missing or reordered')
    if events[0].get('kind')!='registry_created':
        raise ValueError('registry has no creation binding')
    return events


def _pending_snapshot(reader, raw):
    if raw.get('diagnostic_status') not in ('pending','interrupted'):
        return None
    binding=raw.get('optimizer_snapshot') or {}
    stopped=reader.json(binding.get('file'),_sha(binding.get('sha256')))
    if (stopped.get('optimizer_completed') is not True or stopped.get('stop_reason')=='interrupted'
            or stopped.get('diagnostic_status')!='pending' or not stopped.get('diagnostic_state')):
        raise ValueError('diagnostic repair has no completed stopped optimizer')
    if any(raw.get(k)!=stopped.get(k) for k in SNAPSHOT_KEYS):
        raise ValueError('stopped optimizer snapshot changed')
    return binding


def _inventory(generation, source, reader):
    rows=[]
    for job in generation['jobs']:
        attempts=[];completed=[]
        for stored in generation['attempts']:
            if stored['job']!=job:continue
            created=stored['created']
            a={'attempt_id':stored['attempt_id'],'job':job,'suffix':created.get('suffix'),
               'created_time':created.get('time'),'recovery_reason':created.get('recovery_reason')}
            for step in ('dispatched','finished','failed'):
                if step in stored:a[step]=stored[step]
            raw=stored['trace']
            if stored['terminal_bound']:
                if 'host_contaminated' in raw and raw['host_contaminated'] is not stored['host_contaminated']:
                    raise ValueError('trace and parent host verdict disagree')
                a['outcome']={key:raw.get(key) for key in OUTCOME_KEYS}
                binding=_pending_snapshot(reader,raw)
                if raw['optimizer_completed'] and stored['host_contaminated'] is False:
                    eligible={'attempt_id':a['attempt_id'],'trace':stored['trace_file'],
                              'trace_sha256':stored['trace_sha256']}
                    if binding:
                        a['diagnostic_pending_snapshot']=binding;eligible['optimizer_snapshot']=binding
                    completed.append((eligible,binding is not None))
            elif raw is not None:
                a['unfinalized_trace']={'file':stored['trace_file'],'sha256':stored['trace_sha256'],
                    'optimizer_completed':raw['optimizer_completed'],'ledger':raw.get('ledger'),
                    'diagnostic_status':raw.get('diagnostic_status')}
            elif stored.get('missing_trace') and stored.get('finished',{}).get('result',{}).get('status')=='ran':
                a['outcome']={'optimizer_completed':False,'missing_trace':True,
                              'reason':'no trace published before process exit'}
            attempts.append(a)
        selected,pending=completed[0] if completed else (None,False)
        rows.append({'job':job,'attempts':attempts,'eligible_completion':selected,
            'disposition':'completed_optimizer_diagnostic_pending' if pending else
                          'completed_preserve' if selected else 'unreconciled'})
    return {'source':source,'owner_active':False,'ready':True,'incomplete_tail_bytes':0,
            'journal_sha256':generation['file_sha256']['events.jsonl'],
            'bindings_sha256':generation['file_sha256']['bindings.json'],'jobs':rows}


def _diagnostics(generation, source, readers, stack):
    attempts={}
    for event in generation['events']:
        kind=event['kind']
        if kind=='diagnostic_attempt_created':attempts[event['sequence']]={'created':event}
        elif kind in ('diagnostic_attempt_dispatched','diagnostic_attempt_finished','diagnostic_attempt_failed'):
            aid=event.get('attempt_id');step=kind.removeprefix('diagnostic_attempt_')
            if (type(aid) is not int or aid not in attempts or step in attempts[aid]
                    or 'finished' in attempts[aid] or 'failed' in attempts[aid]):
                raise ValueError('unknown or duplicate diagnostic transition')
            attempts[aid][step]=event
    rows=[]
    root=Path(generation['local_directory'])
    for aid,attempt in attempts.items():
        created=attempt['created'];action=created['action'];result=attempt.get('finished',{}).get('result') or {}
        row={'optimizer_attempt':action['optimizer']['key'],'attempt':f'{source}#diagnostic-{aid}',
             'source':source,'status':'pending','events':attempt}
        name=created['directory']
        if type(name) is not str or Path(name).name!=name or name in ('.','..'):
            raise ValueError('diagnostic directory escapes generation')
        directory=root/name
        if directory.exists():
            directory=_directory(root,name)
            reader=Archive(directory);readers.append(reader)
            stack.enter_context(reader.inactive_owner())
            events=_events(reader.read('events.jsonl'))
            ends=[e for e in events if e['kind']=='diagnostic_finished']
            terminal=directory/'terminal.json'
            if terminal.exists():
                if len(ends)!=1 or ends[0].get('result',{}).get('file')!='terminal.json':
                    raise ValueError('diagnostic terminal lacks one worker binding')
                raw=reader.json('terminal.json',ends[0]['result']['sha256'])
                if result.get('terminal') and result['terminal']!={'file':name+'/terminal.json',
                                                                'sha256':reader.hashes['terminal.json']}:
                    raise ValueError('parent diagnostic binding changed')
                inputs=reader.json('inputs.json',raw.get('inputs',{}).get('sha256'))
                if (raw.get('kind')!='diagnostic_only' or raw.get('optimizer_reexecuted') is not False
                    or raw.get('counted_in_run_budget') is not False or raw.get('attempt_id')!=row['attempt']
                    or raw.get('inputs')!={'file':'inputs.json','sha256':reader.hashes['inputs.json']}
                    or inputs.get('session')!=generation['session'] or inputs.get('attempt_id')!=row['attempt']
                    or inputs.get('snapshot')!={'path':str(Path(action['optimizer']['source'])/action['optimizer']['optimizer_snapshot']['file']),
                                               'sha256':action['optimizer']['optimizer_snapshot']['sha256']}
                    or inputs.get('job')!={k:action['job'][k] for k in ('method','space','start','repeat')}):
                    raise ValueError('diagnostic changed its stopped optimizer/session binding')
                row.update(status=raw['status'],result=raw,
                    terminal={'file':str(directory/'terminal.json'),'sha256':reader.hashes['terminal.json']})
            elif result.get('terminal') or ends:
                raise ValueError('bound diagnostic terminal is missing')
        elif result.get('terminal'):
            raise ValueError('bound diagnostic directory is missing')
        rows.append(row)
    return rows


def _external(plan, canonical_registry, local_registry, readers):
    mapped=copy.deepcopy(plan.get('external_events',{}))
    for key,event in mapped.items():
        if type(event.get('evidence')) is not list or not event['evidence']:
            raise ValueError('external event has no evidence')
        for item in event['evidence']:
            digest=_sha(item.get('sha256'))
            expected=str(Path(canonical_registry)/'external-evidence'/(digest+'.bin'))
            if item.get('path')!=expected:
                raise ValueError('external event is outside the frozen registry evidence store')
            folder=_directory(local_registry,'external-evidence')
            reader=Archive(folder);readers.append(reader);reader.read(digest+'.bin',digest)
            item['path']=str(folder/(digest+'.bin'))
    return mapped


def _policy(inventories, repairs, external, harness, expected_hashes):
    harness=Path(harness).resolve(strict=True)
    required={'recovery_policy.py','session_journal.py','process_identity.py'}
    if set(expected_hashes)!=required:
        raise ValueError('pin all recovery-policy dependencies')
    sources={name:(harness/name).read_bytes() for name in required}
    for name,value in expected_hashes.items():
        if hashlib.sha256(sources[name]).hexdigest()!=_sha(value):
            raise ValueError('recovery-policy source changed')
    program="""import json,sys,types
value=json.load(sys.stdin)
for name in ('process_identity','session_journal','recovery_policy'):
 module=types.ModuleType(name)
 module.__file__=sys.argv[1]+'/'+name+'.py'
 sys.modules[name]=module
 exec(compile(value['sources'][name+'.py'],module.__file__,'exec'),module.__dict__)
print(json.dumps(sys.modules['recovery_policy'].plan(value['inventories'],external_events=value['external'],diagnostic_attempts=value['repairs']),allow_nan=False))
"""
    result=subprocess.run([sys.executable,'-I','-B','-c',program,str(harness)],
        input=json.dumps({'inventories':inventories,'repairs':repairs,'external':external,
                          'sources':{name:raw.decode('utf-8') for name,raw in sources.items()}},allow_nan=False),
        text=True,capture_output=True,timeout=60)
    if result.returncode:
        raise ValueError('pinned recovery policy refused evidence: '+result.stderr.strip())
    # Refuse source changes during execution as well.
    for name,value in expected_hashes.items():
        if hashlib.sha256((harness/name).read_bytes()).hexdigest()!=value:
            raise ValueError('recovery-policy source changed during reconciliation')
    return _json(result.stdout)


def _actions(policy):
    return {phase:[(a['job'],a['reason'],a.get('replaces'),
                   a.get('optimizer')) for a in policy[phase]]
            for phase in ('remaining_registered','recovery_tail','diagnostics')}


def reconcile(original, *, registry, harness, policy_source_sha256):
    """Load a complete supplied chain and return the pinned policy's dispositions.

    registry=None explicitly means no registry was included in the capture. Local
    absence is checked; completeness of a copied capture remains a caller obligation.
    The report never grants scientific selection or launches work. Unresolved jobs,
    unfinished diagnostics and unsealed generations remain explicit.
    """
    first=load_generation(original)
    canonical=first['bindings']['session_directory']
    canonical_registry=str(Path(canonical).with_name(Path(canonical).name+'.recoveries'))
    if first['bindings'].get('recovery_registry')!=canonical_registry:
        raise ValueError('original session changed its registered recovery location')
    generations=[first];sources=[canonical];plans=[];seals=[];registry_record=None
    readers=[Archive(first['local_directory'])]
    # Bind everything the generation loader consumed to this longer read.
    readers[0].hashes.update(first['file_sha256'])
    with ExitStack() as stack:
        if registry is None:
            if Path(original).with_name(Path(original).name+'.recoveries').exists():
                raise ValueError('an existing recovery registry was omitted')
        else:
            reader=Archive(registry);readers.append(reader)
            stack.enter_context(reader.inactive_owner())
            events=_registry_events(reader.read('events.jsonl'))
            origin=reader.json('origin.json',events[0]['origin_sha256'])
            if origin!={'schema':1,'source':canonical,'journal_sha256':first['file_sha256']['events.jsonl'],
                        'bindings_sha256':first['file_sha256']['bindings.json']}:
                raise ValueError('registry origin changed')
            reservations=[];finished={}
            for event in events[1:]:
                if event['kind']=='generation_reserved':
                    index=len(reservations)+1
                    if type(event.get('index')) is not int or event['index']!=index or event.get('directory')!=f'generation-{index:04d}':
                        raise ValueError('missing or reordered registry generation')
                    reservations.append(event)
                elif event['kind']=='generation_finished':
                    index=event.get('index')
                    if type(index) is not int or not 1<=index<=len(reservations) or index in finished:
                        raise ValueError('unknown or duplicate registry completion')
                    finished[index]=event
                else:raise ValueError('unknown recovery registry transition')
            if {p.name for p in reader.root.glob('generation-*')}!={e['directory'] for e in reservations}:
                raise ValueError('missing or unregistered recovery directory')
            for event in reservations:
                index=event['index'];previous=generations[-1];previous_source=sources[-1]
                if event['previous']!=previous_source or event['previous_journal_sha256']!=previous['file_sha256']['events.jsonl']:
                    raise ValueError('recovery chain branched or prior journal changed')
                bound=event['plan']
                if bound.get('file')!=f'plan-{index:04d}.json':raise ValueError('wrong reserved plan name')
                plans.append(reader.json(bound['file'],bound['sha256']))
                generation=load_generation(_directory(reader.root,event['directory']))
                if any(generation['bindings'].get(k)!=v for k,v in first['bindings'].items()):
                    raise ValueError('generation changed original frozen bindings')
                expected_link={'registry':canonical_registry,'original':canonical,'index':index,
                               'previous':previous_source,'plan':bound}
                if generation['bindings'].get('recovery')!=expected_link:
                    raise ValueError('generation changed its reserved recovery link')
                observed={k:generation['file_sha256'][name] for k,name in
                          (('journal_sha256','events.jsonl'),('bindings_sha256','bindings.json'))}
                seal=finished.get(index)
                gr=Archive(generation['local_directory']);gr.hashes.update(generation['file_sha256']);readers.append(gr)
                if seal:
                    if any(seal.get(k)!=v for k,v in observed.items()):raise ValueError('sealed generation changed')
                    summary=seal.get('summary')
                    if not summary or summary.get('file')!='manifest.json':raise ValueError('sealed generation has no summary')
                    generation['manifest']=gr.json('manifest.json',summary['sha256'])
                generations.append(generation);sources.append(str(Path(canonical_registry)/event['directory']));seals.append(seal)
            registry_record={'source_directory':canonical_registry,'local_directory':str(reader.root),
                             'events':events,'origin':origin}
        inventories=[];repairs=[];external={};original_external={}
        for index,(generation,source) in enumerate(zip(generations,sources)):
            if any(e['kind']=='recovery_verification_failed' for e in generation['events']):
                raise ValueError('generation contains a failed dispatch verification')
            gr=Archive(generation['local_directory']);gr.hashes.update(generation['file_sha256']);readers.append(gr)
            if index:
                reserved=plans[index-1]
                for key,event in reserved.get('external_events',{}).items():
                    if key in original_external and original_external[key]!=event:
                        raise ValueError('external attestation changed across generations')
                    original_external[key]=event
                external.update(_external(reserved,canonical_registry,Path(registry_record['local_directory']),readers))
                expected=_policy(inventories,repairs,external,harness,policy_source_sha256)
                if _actions(reserved)!=_actions(expected):raise ValueError('reserved plan differs from prior eligible history')
                allowed={a['job']['name']:a['reason'] for phase in ('remaining_registered','recovery_tail') for a in expected[phase]}
                for a in generation['attempts']:
                    if a['job']['name'] not in allowed or a['created'].get('recovery_reason')!=allowed[a['job']['name']]:
                        raise ValueError('optimizer attempt was not authorized by its reserved recovery plan')
                allowed_diagnostics=_actions(expected)['diagnostics']
                for event in generation['events']:
                    if event['kind']=='diagnostic_attempt_created':
                        action=event['action']
                        signature=(action['job'],action['reason'],action.get('replaces'),action.get('optimizer'))
                        if signature not in allowed_diagnostics:
                            raise ValueError('diagnostic was not authorized by its reserved recovery plan')
            inventories.append(_inventory(generation,source,gr))
            repairs.extend(_diagnostics(generation,source,readers,stack))
        result=_policy(inventories,repairs,external,harness,policy_source_sha256)
        for reader in readers:reader.verify_stable()
        closed=(len(result['jobs'])==100 and all(row['selected_completion'] is not None for row in result['jobs'])
            and not any(result[key] for key in ('remaining_registered','recovery_tail','diagnostics'))
            and result['unresolved_job_count']==0 and all(seals)
            and generations[-1]['manifest'] is not None and generations[-1]['manifest'].get('complete') is True)
        return {'schema':1,'generations':generations,'source_directories':sources,'registry':registry_record,
            'policy':result,'diagnostic_attempts':repairs,'original_external_events':original_external,
            'all_registered_jobs_resolved':closed,'can_select':False,'can_launch':False,
            'policy_source_sha256':dict(policy_source_sha256),
            'read_evidence':[{'directory':str(r.root),'file_sha256':dict(r.hashes)} for r in readers],
            'scope':'Attempt reconciliation only; capture completeness, remote process absence and scientific/design provenance remain caller obligations.'}
