"""Synthetic scientific assembly on real frozen setup; no main outcome evidence.

Internal assembly tests isolate scientific checks. The public-entry roundtrip
also verifies capture and setup, using synthetic build/runtime and optimizer
evidence. Neither is a closed main study or measured optimization result.
"""
import copy
import hashlib
import gzip
import importlib
import json
import math
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest

HERE=Path(__file__).resolve().parent;sys.path.insert(0,str(HERE))
import producer_source
import study_selection
import test_capture_archive
import closed_capture
from test_trace_archive import write, digest
import test_setup_evidence
from test_trace_archive import jobs
from validation_evidence import REPAIR_LABELS


class Reader:
    def __init__(self,root,source):
        self.root=root;self.source=source;self.manifest={'inventory':{'files':{}}};self.stable_calls=0
    def mapped(self,original):return self.root/Path(original).relative_to(self.source)
    def path(self,relative):
        path=self.root/relative
        if hashlib.sha256(path.read_bytes()).hexdigest()!=self.manifest['inventory']['files'][relative]['sha256']:
            raise ValueError('mesh bytes changed')
        return path
    def stable(self):self.stable_calls+=1
    def mesh(self,attempt=1):
        name=f'trace-{attempt}.meshes/validation/generated.msh';path=self.root/name
        path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(b'synthetic mesh fixture, not FEM evidence')
        digest=hashlib.sha256(path.read_bytes()).hexdigest()
        self.manifest['inventory']['files'][name]={'sha256':digest};return digest,name


class StudySelectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        test_setup_evidence.SetupEvidenceTests.setUpClass()
        cls.b=test_setup_evidence.SetupEvidenceTests
        cls.module_context=producer_source.committed_modules(cls.b.inputs['anchor'].parents[2],cls.b.commit,('ledger',))
        cls.module_context.__enter__();cls.Ledger=importlib.import_module('ledger').Ledger
    @classmethod
    def tearDownClass(cls):cls.module_context.__exit__(None,None,None);cls.b.tearDownClass()
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.setup=study_selection.setup_evidence.check(copy.deepcopy(self.b.original),frozen=self.b.frozen,
            inputs=self.b.inputs,source=self.b.source,git_repo=self.b.inputs['anchor'].parents[2],reviewed_commit=self.b.commit)
        self.starts=json.loads(self.b.inputs['starts'].read_text());self.session=self.b.frozen['session']
        self.source=self.b.original['bindings.json']['session_directory']
        self.reader=Reader(Path(self.temp.name).resolve(),self.source)
        self.generation={'original_session_directory':self.source,'session':self.session,'jobs':jobs(),'attempts':[]}
        policy=[]
        for i,job in enumerate(jobs()):
            a=self.attempt(job,i+1,validations=i==0);self.generation['attempts'].append(a)
            policy.append({'job':job,'selected_completion':{'key':self.source+'#'+str(i+1),
                'trace':a['trace_file'],'trace_sha256':a['trace_sha256']}})
        self.capture={'frozen':self.b.frozen,'manifest_sha256':'a'*64,'input_paths':{'validation_mesh':'inputs/validation.msh'},
            'chain':{'all_registered_jobs_resolved':True,'generations':[self.generation],'policy':{'jobs':policy}}}
    def attempt(self,job,aid,*,validations=False):
        model=self.session['models']['L0' if job['method_letter'] in 'DE' else 'L1']
        ledger=self.Ledger(self.session['unit_seconds'],120.,weights=model['weights'],clock=lambda:0.)
        ledger.charge('overhead',1.,note='synthetic startup')
        ready=copy.deepcopy(next(e['ready'] for e in self.b.original['anchor_preflight.json']['sessions'] if e['space']==job['space']))
        ready['train_msh_sha256']=model['train_mesh_sha256'];ready['session']['training']['level']=model['train_level']
        ready['session']['registered']=model['train_level']==1
        anchor=copy.deepcopy(self.setup['effective_anchor']);binding=self.b.frozen['inputs']['anchor']
        anchor.update(targets={n:r*anchor['F0'] for n,r in (('T50',.5),('T90',.1),('T99',.01))},
            mean_floor=1.0025*anchor['mean0_full_current'],source=binding['path'],file_sha256=binding['sha256'],
            file_values={'F0':self.setup['canonical_anchor']['F0'],'mean0':self.setup['canonical_anchor']['mean0_full_current']})
        trace={'training_model':model,'space':job['space'],'stop_reason':'error','optimizer_completed':True,
               'evaluator_ready':ready,'anchor':anchor,'x_start':self.starts['starts'][job['start']][job['space']],
               'validations':[],'events':[]}
        if validations:
            for index,f in enumerate((.00001,.00002)):
                ledger.charge('validation',4.,60,note='synthetic completed validation')
                g=copy.deepcopy(next(e for e in self.b.original['start_preflight.json']['sessions'] if e['space']==job['space'])['checks'][0]['reply']['geometry'])
                mean=anchor['mean_floor']+.01;delta=max(.1*f,2.5e-7)
                quality={'inverted':0,'min_angle_deg':10.,'max_angle_deg':130.,'min_area_ratio':1.,'max_area_ratio':1.}
                trace['validations'].append({'index':index,'status':'ok','seconds':ledger.charged_seconds(),
                    'wall_fe':ledger.total_fe(),'solve_fe':ledger.solve_fe(),'credited':True,
                    'x':self.starts['starts']['lhs'+str(index+1)][job['space']],'F_val':f,'mean_full':mean,
                    'fractions':[0.,.5,1.],'stds':[math.sqrt(f),0.,0.],'means':[0.,.4,mean],
                    'delta_F':delta,'F_upper':f+delta,'mean_floor':anchor['mean_floor'],'mean_ok':True,
                    'admissible':True,'h_nonnegative':True,'min_h':g['min_h'],'geometry':g,
                    'attained':{n:f+delta<=t for n,t in anchor['targets'].items()},
                    'mesh':{'kind':'moved','quality':quality},'msh_sha256':anchor['msh_sha256']})
        ledger.clock=lambda:ledger.charged_seconds();trace['ledger']=ledger.to_json()
        trace['in_run_wall_seconds']=ledger.charged_seconds()
        return {'job':job,'attempt_id':aid,'trace_file':f'trace-{aid}.json','trace_sha256':hashlib.sha256(str(aid).encode()).hexdigest(),
                'terminal_bound':True,'host_contaminated':False,'trace':trace}
    def assemble(self):
        return study_selection._assemble(self.capture,self.setup,self.reader,ledger_type=self.Ledger,starts=self.starts)
    def test_full_matrix_preserves_earlier_validation_from_failed_run_and_all_missing_cells(self):
        before=copy.deepcopy((self.capture,self.setup));out=self.assemble()
        self.assertEqual((self.capture,self.setup),before)
        self.assertEqual(len(out['attempts']),100);self.assertEqual(len(out['selection']['cells']),40)
        self.assertTrue(out['can_select']);self.assertFalse(out['can_launch'])
        cells=out['selection']['cells'];self.assertEqual(cells[0]['record_id'],self.source+'#1:0')
        self.assertEqual(sum(c['status']=='selected' for c in cells),4)
        self.assertEqual(len(out['selection']['records']),2);self.assertEqual(len(out['plan']['jobs']),21)
        self.assertEqual(len(out['designs']),3);json.dumps(out,allow_nan=False)
    def test_contaminated_and_unselected_attempts_remain_excluded(self):
        a=self.attempt(jobs()[0],101,validations=True);a['host_contaminated']=True
        self.generation['attempts'].append(a);out=self.assemble()
        self.assertEqual(len(out['attempts']),101);self.assertEqual(len(out['selection']['records']),4)
        rows=[r for r in out['selection']['records'] if '#101:' in r['record']['run_id']+':']
        self.assertEqual(len(rows),2)
        self.assertTrue(all(not r['eligible'] for r in rows))
    def test_missing_completion_and_wrong_trace_identity_refused(self):
        self.capture['chain']['policy']['jobs'][0]['selected_completion']['trace_sha256']='b'*64
        with self.assertRaisesRegex(ValueError,'terminal attempt'):self.assemble()
    def test_incomplete_study_refused(self):
        self.capture['chain']['all_registered_jobs_resolved']=False
        with self.assertRaisesRegex(ValueError,'complete resolved'):self.assemble()
    def test_changed_start_anchor_and_training_configuration_refused(self):
        a=self.generation['attempts'][0];original=copy.deepcopy(a['trace'])
        for name in ('start','anchor','training'):
            a['trace']=copy.deepcopy(original);t=a['trace']
            if name=='start':t['x_start']=[0.]*8
            elif name=='anchor':t['anchor']['F0']*=.9
            else:t['evaluator_ready']['session']['training']['mode']='h93'
            with self.subTest(name=name),self.assertRaises(ValueError):self.assemble()
    def test_changed_ledger_or_validation_prefix_refused(self):
        a=self.generation['attempts'][0];original=copy.deepcopy(a['trace'])
        for name in ('ledger','prefix','solve'):
            a['trace']=copy.deepcopy(original);t=a['trace']
            if name=='ledger':t['ledger']['entries'][0]['seconds']+=.1
            elif name=='prefix':t['validations'][0]['seconds']+=.1
            else:t['validations'][0]['solve_fe']+=1.
            with self.subTest(name=name),self.assertRaises(ValueError):self.assemble()
    def test_unavailable_events_are_retained_without_completed_validations(self):
        event={'kind':'validation_unavailable','error':'synthetic solver failure'}
        self.generation['attempts'][1]['trace']['events']=[event]
        out=self.assemble()
        self.assertEqual(out['attempts'][1]['validation_unavailable_events'],[event])
        self.assertEqual(out['attempts'][1]['validations'],[])

    def test_l0_baselines_use_native_false_registration_with_primary_l2_validation(self):
        for position in (3,4):
            original=self.generation['attempts'][position]
            a=self.attempt(jobs()[position],original['attempt_id'],validations=True)
            self.generation['attempts'][position]=a
            out=self.assemble()
            self.assertEqual(len(out['attempts'][position]['validations']),2)
            a['trace']['evaluator_ready']['session']['registered']=True
            with self.assertRaisesRegex(ValueError,'registration flag'):self.assemble()
            self.generation['attempts'][position]=original

    def test_l1_registration_flag_cannot_be_false(self):
        self.generation['attempts'][0]['trace']['evaluator_ready']['session']['registered']=False
        with self.assertRaisesRegex(ValueError,'registration flag'):self.assemble()

    def test_changed_credit_is_refused(self):
        self.generation['attempts'][0]['trace']['validations'][0]['credited']=False
        with self.assertRaises(ValueError):self.assemble()
    def fresh(self):
        r=self.generation['attempts'][0]['trace']['validations'][0];digest,path=self.reader.mesh()
        q=copy.deepcopy(r['mesh']['quality']);moved=copy.deepcopy(q);moved['min_angle_deg']=4.
        r.update(msh_sha256=digest,mesh={'kind':'fresh','quality':q,'moved_quality':moved,'msh_sha256':digest,
            'trigger':{'kind':'min_angle','min_angle_deg':4.,'threshold_deg':5.},
            'attempts':[{'attempt':0,'label':REPAIR_LABELS[0],'outcome':'passed','detail':None,
                         'msh_sha256':digest,'mesh_s':.1,'setup_s':.2}]})
        return path
    def test_fresh_mesh_bytes_must_belong_to_originating_attempt(self):
        path=self.fresh();out=self.assemble();self.assertTrue(out['attempts'][0]['validations'][0]['meshes'])
        item=self.reader.manifest['inventory']['files'].pop(path)
        self.reader.manifest['inventory']['files'][path.replace('trace-1.','trace-2.')]=item
        with self.assertRaisesRegex(ValueError,'originating attempt'):self.assemble()
    def scientific_capture(self):
        # Synthetic build/runtime and outcomes; real immutable physics definitions.
        # This exercises the public chain without claiming measured execution.
        root=self.reader.root/'roundtrip';root.mkdir()
        base=test_capture_archive.fixture(root)
        repo=root/'producer';main=root/'main';build=root/'build'
        base_frozen=json.loads((main/'recovery-inputs.json').read_text())
        old=self.b.frozen;bindings=copy.deepcopy(self.b.original['bindings.json'])
        substitutions={self.source+'.recoveries':str(main)+'.recoveries',old['inputs']['binary']['path']:str(build/'icem_harness_eval'),
            old['inputs']['binary']['sha256']:digest(build/'icem_harness_eval'),
            old['inputs']['build_receipt']['path']:str(build/'build-receipt.json'),
            old['inputs']['build_receipt']['sha256']:digest(build/'build-receipt.json')}
        for level in (0,1):
            name='training_L'+str(level);content=('synthetic mesh L'+str(level)).encode()
            substitutions[old['inputs'][name]['sha256']]=hashlib.sha256(content).hexdigest()
        def change(v):
            if isinstance(v,dict):return {change(k):change(x) for k,x in v.items()}
            if isinstance(v,list):return [change(x) for x in v]
            if isinstance(v,str):
                if v in substitutions:return substitutions[v]
                for before,after in ((self.source,str(main)),(old['source']['repo'],str(repo))):
                    if v==before or v.startswith(before+'/'):return after+v[len(before):]
            return v
        records=change(copy.deepcopy(self.b.original));frozen=change(copy.deepcopy(old))
        frozen['source']=base_frozen['source'];frozen['runtime']=base_frozen['runtime']
        records['bindings.json']['provenance']['source']=frozen['source']
        records['bindings.json']['provenance']['runtime']=frozen['runtime']
        for name,item in frozen['inputs'].items():
            path=Path(item['path']);path.parent.mkdir(parents=True,exist_ok=True)
            if name in ('binary','build_receipt'):continue
            if name.startswith('training_'):content=('synthetic mesh '+name[-2:]).encode()
            elif name=='validation_mesh':content=gzip.decompress((HERE/'smoke/native_binding_L2.msh.gz').read_bytes())
            elif name in ('anchor','starts'):content=self.b.inputs[name].read_bytes()
            else:raise AssertionError('unhandled input '+name)
            path.write_bytes(content);assert digest(path)==item['sha256'],name
        write(main/'anchor_preflight.json',records['anchor_preflight.json'])
        session=records['session.json'];session['anchor_recomputation']['sha256']=digest(main/'anchor_preflight.json')
        frozen['session']=session
        write(main/'recovery-inputs.json',frozen)
        for name in ('session.json','start_preflight.json','calibration_primary.json','calibration_secondary.json'):
            write(main/name,records[name])
        write(main/'queue.json',{'jobs':jobs()})
        bound=records['bindings.json']
        bound['files']={n:digest(main/n) for n in bound['files']};write(main/'bindings.json',bound)
        events=[json.loads(line) for line in (main/'events.jsonl').read_text().splitlines()]
        selected={}
        for e in events:
            if e['kind']=='session_ready':e['bindings_sha256']=digest(main/'bindings.json')
            elif e['kind']=='attempt_created':selected[e['sequence']]=e['job']
            elif e['kind']=='attempt_finished':
                aid=e['attempt_id'];job=selected[aid]
                t=change(self.attempt(job,aid,validations=job['position']==0)['trace'])
                t.update({k:job[k] for k in ('method','space','start','repeat')})
                t.update(label='main',session=session['session_id'],unit_seconds=session['unit_seconds'],
                         budget_fe=session['budget_fe'],order_position=job['position'])
                name=e['result']['trace'];write(main/name,t);e['result']['trace_sha256']=digest(main/name)
        (main/'events.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in events))
        verified={'reviewed_commit':self.b.commit,'files':self.b.source['files']}
        proof={'verified':True,'path':str(build/'build-receipt.json'),'sha256':digest(build/'build-receipt.json'),
            'binary_sha256':digest(build/'icem_harness_eval'),'source_commit':self.b.commit}
        observations={'observed':frozen,'build_verification':proof,
            'binding_verification':{'verified':True,'schema':1,'can_launch':False}}
        with producer_source.committed_modules(HERE.parents[2],self.b.commit,study_selection.capture_archive.PROCESS_MODULES):
            verifier=importlib.import_module('process_verification')
            result=closed_capture._export(main,repo,root/'scientific_capture',frozen,verified,
                chain_probe=lambda source,registry:closed_capture._chain(source,registry,HERE.parents[2],verified),
                binding_probe=lambda:copy.deepcopy(observations),
                process_probe=lambda chain,build:closed_capture._processes(chain,build,verifier))
        return root/'scientific_capture',result

    def test_public_capture_setup_validation_selection_chain(self):
        path,receipt=self.scientific_capture()
        # Release the fixture's reviewed ledger module so the public entry must
        # load its own reviewed implementation, with no module injection.
        self.module_context.__exit__(None,None,None)
        try:
            before={p:(p.read_bytes(),p.stat().st_mtime_ns) for p in path.rglob('*') if p.is_file()}
            out=study_selection.load(path,manifest_sha256=receipt['sha256'],git_repo=HERE.parents[2],
                reviewed_commit=self.b.commit,capture_tool_commit=test_capture_archive.CAPTURE_TOOL)
            self.assertTrue(out['can_select']);self.assertFalse(out['can_launch'])
            self.assertEqual(len(out['attempts']),100);self.assertEqual(len(out['plan']['jobs']),21)
            output=path.parent/'frozen-selection.json'
            command=[sys.executable,'-I',str(HERE/'study_selection.py'),'--capture',str(path),
                '--manifest-sha256',receipt['sha256'],'--git-repo',str(HERE.parents[2]),
                '--reviewed-commit',self.b.commit,'--capture-tool-commit',test_capture_archive.CAPTURE_TOOL,
                '--out',str(output)]
            result=subprocess.run(command,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(output.read_text()),out)
            self.assertEqual(json.loads(result.stdout)['sha256'],digest(output))
            repeated=subprocess.run(command,capture_output=True,text=True)
            self.assertNotEqual(repeated.returncode,0)
            for p,(raw,mtime) in before.items():
                self.assertEqual(p.read_bytes(),raw);self.assertEqual(p.stat().st_mtime_ns,mtime)
        finally:
            type(self).module_context=producer_source.committed_modules(HERE.parents[2],self.b.commit,('ledger',))
            self.module_context.__enter__();type(self).Ledger=importlib.import_module('ledger').Ledger

    def test_changed_mesh_bytes_refused(self):
        path=self.fresh();(self.reader.root/path).write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError,'mesh bytes'):self.assemble()


if __name__=='__main__':unittest.main(verbosity=2)
