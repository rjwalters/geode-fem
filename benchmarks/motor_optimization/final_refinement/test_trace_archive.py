"""On-disk synthetic archive checks; no optimization or FEM evidence."""
import copy
import fcntl
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest import mock
sys.path.insert(0, str(Path(__file__).resolve().parent))
import trace_archive as archive


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, data):
    path.write_text(json.dumps(data, allow_nan=False)+'\n')


def jobs():
    result = []
    names = ('slsqp','cobyqa','surrogate','cobyqa_l0','slsqp_l0')
    starts = ('theta0','lhs1','lhs2','lhs3','lhs4')
    for repeat in (1,2):
        for space in ('a','b'):
            for start_index,start in enumerate(starts):
                order = [(start_index+i)%5 for i in range(5)]
                if repeat == 2: order.reverse()
                for order_index,method_index in enumerate(order):
                    method=names[method_index]
                    result.append(dict(position=len(result), repeat=repeat, space=space,
                        start=start, start_index=start_index, method=method,
                        method_letter='ABCDE'[method_index], order_index=order_index,
                        name=f'{method}_{space}_{start}_r{repeat}'))
    return result


class Fixture:
    def __init__(self, root):
        self.root=root; root.mkdir()
        (root/'owner.lock').write_text('')
        self.queue=jobs()
        self.session={'label':'main','session_id':'original-main','unit_seconds':2.,'budget_fe':120.,
            'models':{'L0':{'train_mesh_sha256':'0'*64},'L1':{'train_mesh_sha256':'1'*64}}}
        self.events=[]
        self.emit('session_created')
        write(root/'session.json',self.session)
        write(root/'queue.json',{'jobs':self.queue})
        for name in ('calibration_primary.json','calibration_secondary.json'):
            write(root/name,{'accepted_for_dispatch':True,'calibration':{'unit_seconds':2.}})
        write(root/'recovery-inputs.json',{'fixture':'retained for downstream source/runtime checks'})
        names=('session.json','queue.json','calibration_primary.json','calibration_secondary.json','recovery-inputs.json')
        self.bindings={'session_directory':'/cloud/experiment/main','unit_seconds':2.,
                       'files':{n:digest(root/n) for n in names}}
        write(root/'bindings.json',self.bindings)
        self.emit('session_ready',bindings_sha256=digest(root/'bindings.json'))

    def emit(self,kind,**fields):
        seq=len(self.events);self.events.append(dict(sequence=seq,kind=kind,**fields));self.save();return seq

    def save(self):
        (self.root/'events.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in self.events))

    def rebind(self,name):
        self.bindings['files'][name]=digest(self.root/name)
        write(self.root/'bindings.json',self.bindings)
        self.events[1]['bindings_sha256']=digest(self.root/'bindings.json');self.save()

    def add(self,index=0,*,complete=True,terminal=True,contaminated=False,reason='error',validations=True):
        job=self.queue[index];aid=self.emit('attempt_created',job=job,suffix='')
        name=f'trace-{aid}.json';self.emit('attempt_dispatched',attempt_id=aid,trace=name)
        anchor={'F0':1.,'mean0_full_current':2.,'level':2,'msh_sha256':'2'*64}
        level='L0' if job['method_letter'] in 'DE' else 'L1'
        raw={k:job[k] for k in ('method','space','start','repeat')}
        raw.update(label='main',session=self.session['session_id'],unit_seconds=2.,budget_fe=120.,order_position=job['position'],
            optimizer_completed=complete,stop_reason=reason if complete else 'interrupted',
            validations=[{'index':0,'F_val':.1},{'index':1,'F_val':.2}] if validations else [],
            training_model=self.session['models'][level],anchor=anchor,
            evaluator_ready={'anchor':anchor,'train_msh_sha256':self.session['models'][level]['train_mesh_sha256'],
                             'validation_msh_sha256':'2'*64})
        write(self.root/name,raw)
        if terminal:
            self.emit('attempt_finished',attempt_id=aid,result={'status':'ran','exit':1 if complete else 143,
                'trace':name,'trace_sha256':digest(self.root/name),'host_contaminated':contaminated})
        return name

    def replace_trace(self,name,mutate):
        raw=json.loads((self.root/name).read_text());mutate(raw);write(self.root/name,raw)
        for event in self.events:
            if event.get('result',{}).get('trace')==name:
                event['result']['trace_sha256']=digest(self.root/name)
        self.save()


class TraceArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.fixture=Fixture(Path(self.temp.name)/'main');self.root=self.fixture.root

    def test_retains_all_methods_scientific_failures_and_contaminated_attempts_read_only(self):
        for i in range(5):self.fixture.add(i,contaminated=i==3)
        before={p.name:(p.read_bytes(),p.stat().st_mtime_ns) for p in self.root.iterdir()}
        out=archive.load_generation(self.root)
        self.assertEqual(len(out['jobs']),100);self.assertEqual(len(out['attempts']),5)
        self.assertEqual([a['trace']['stop_reason'] for a in out['attempts']],['error']*5)
        self.assertTrue(out['attempts'][3]['host_contaminated'])
        self.assertEqual(sum(len(a['trace']['validations']) for a in out['attempts']),10)
        self.assertFalse(out['can_select']);self.assertFalse(out['can_launch'])
        self.assertEqual(before,{p.name:(p.read_bytes(),p.stat().st_mtime_ns) for p in self.root.iterdir()})
        self.assertFalse(self.root.with_name('main.recoveries').exists())

    def test_copied_archive_keeps_original_identity(self):
        self.fixture.add();copied=Path(self.temp.name)/'copy';shutil.copytree(self.root,copied)
        out=archive.load_generation(copied)
        self.assertEqual(out['original_session_directory'],'/cloud/experiment/main')
        self.assertEqual(out['local_directory'],str(copied.resolve()))

    def test_changed_trace_bytes_are_refused_even_if_json_equivalent(self):
        name=self.fixture.add();p=self.root/name;p.write_text(p.read_text()+' ')
        with self.assertRaisesRegex(ValueError,'changed bound artifact'):archive.load_generation(self.root)

    def test_each_frozen_input_and_binding_hash_is_checked(self):
        for name in (*self.fixture.bindings['files'],'bindings.json'):
            with self.subTest(name=name):
                p=self.root/name;before=p.read_bytes();p.write_bytes(before+b' ')
                with self.assertRaisesRegex(ValueError,'changed bound artifact'):archive.load_generation(self.root)
                p.write_bytes(before)

    def test_missing_or_reordered_jobs_cannot_be_a_closed_main_cohort(self):
        for mutate in (lambda j:j.pop(),lambda j:j.reverse(),lambda j:j[0].update(repeat=True)):
            with self.subTest(mutation=mutate):
                q=jobs();mutate(q);write(self.root/'queue.json',{'jobs':q});self.fixture.rebind('queue.json')
                with self.assertRaises(ValueError):archive.load_generation(self.root)

    def test_rejected_or_different_calibration_is_refused(self):
        for value in ({'accepted_for_dispatch':False,'calibration':{'unit_seconds':2.}},
                      {'accepted_for_dispatch':True,'calibration':{'unit_seconds':3.}}):
            write(self.root/'calibration_secondary.json',value);self.fixture.rebind('calibration_secondary.json')
            with self.assertRaisesRegex(ValueError,'calibration'):archive.load_generation(self.root)

    def test_common_unit_binding_and_session_creation_are_required(self):
        self.fixture.bindings['unit_seconds']=3.
        self.fixture.rebind('session.json')
        with self.assertRaisesRegex(ValueError,'common validation unit'):archive.load_generation(self.root)
        self.fixture.bindings['unit_seconds']=2.
        self.fixture.rebind('session.json')
        self.fixture.events[0]['kind']='setup_started';self.fixture.save()
        with self.assertRaisesRegex(ValueError,'session creation'):archive.load_generation(self.root)

    def test_trace_identity_unit_budget_and_model_cannot_drift(self):
        name=self.fixture.add();original=json.loads((self.root/name).read_text())
        changes=({'method':'cobyqa'},{'space':'b'},{'repeat':True},{'session':'another'},
                 {'unit_seconds':3.},{'budget_fe':121.},{'label':'smoke'},{'order_position':1},
                 {'training_model':{'train_mesh_sha256':'0'*64}})
        for change in changes:
            with self.subTest(change=change):
                self.fixture.replace_trace(name,lambda r:r.update(copy.deepcopy(original),**change))
                with self.assertRaises(ValueError):archive.load_generation(self.root)

    def test_observed_training_and_validation_meshes_and_anchor_are_bound(self):
        name=self.fixture.add(3);original=json.loads((self.root/name).read_text())
        for field,value in (('train_msh_sha256','1'*64),('validation_msh_sha256','0'*64),('anchor',{})):
            with self.subTest(field=field):
                def mutate(r):r.update(copy.deepcopy(original));r['evaluator_ready'][field]=value
                self.fixture.replace_trace(name,mutate)
                with self.assertRaises(ValueError):archive.load_generation(self.root)

    def test_validation_ordinals_and_container_are_checked(self):
        name=self.fixture.add()
        for value in ([{'index':1}],[{'index':0},{'index':0}],[{'index':False}],{}):
            self.fixture.replace_trace(name,lambda r:r.update(validations=value))
            with self.assertRaises(ValueError):archive.load_generation(self.root)

    def test_startup_failure_can_have_no_ready_or_validations(self):
        name=self.fixture.add(validations=False)
        self.fixture.replace_trace(name,lambda r:r.pop('evaluator_ready'))
        self.assertEqual(archive.load_generation(self.root)['attempts'][0]['trace']['validations'],[])

    def test_unfinalized_trace_is_preserved_without_terminal_credit(self):
        self.fixture.add(terminal=False,complete=False)
        out=archive.load_generation(self.root);attempt=out['attempts'][0]
        self.assertFalse(attempt['terminal_bound']);self.assertIsNone(attempt['host_contaminated'])
        self.assertEqual(attempt['trace']['stop_reason'],'interrupted');self.assertFalse(out['can_select'])

    def test_missing_trace_remains_explicit_and_never_successful(self):
        name=self.fixture.add();(self.root/name).unlink()
        result=self.fixture.events[-1]['result'];result.pop('trace_sha256');self.fixture.save()
        attempt=archive.load_generation(self.root)['attempts'][0]
        self.assertTrue(attempt['missing_trace']);self.assertIsNone(attempt['trace'])
        result['exit']=0;self.fixture.save()
        with self.assertRaisesRegex(ValueError,'no trace'):archive.load_generation(self.root)

    def test_interrupted_missing_trace_and_unknown_host_are_preserved(self):
        name=self.fixture.add(terminal=False);(self.root/name).unlink()
        self.assertTrue(archive.load_generation(self.root)['attempts'][0]['missing_trace'])

    def test_duplicate_terminal_or_unknown_attempt_transition_is_refused(self):
        self.fixture.add();last=copy.deepcopy(self.fixture.events[-1]);last.pop('sequence');kind=last.pop('kind')
        self.fixture.emit(kind,**last)
        with self.assertRaisesRegex(ValueError,'terminal'):archive.load_generation(self.root)
        self.fixture.events[-1]['attempt_id']=999;self.fixture.save()
        with self.assertRaisesRegex(ValueError,'no created'):archive.load_generation(self.root)

    def test_torn_duplicate_key_and_overflow_json_are_refused(self):
        p=self.root/'events.jsonl';original=p.read_bytes()
        for raw in (original+b'{',original.replace(b'"sequence": 0',b'"sequence": 0, "sequence": 0'),
                    original.replace(b'"sequence": 0',b'"sequence": 0, "bad": 1e999')):
            p.write_bytes(raw)
            with self.assertRaises(ValueError):archive.load_generation(self.root)
        p.write_bytes(original)

    def test_active_owner_and_missing_lock_are_refused_without_creation(self):
        with (self.root/'owner.lock').open('rb') as stream:
            fcntl.flock(stream,fcntl.LOCK_EX|fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError,'active'):archive.load_generation(self.root)
        (self.root/'owner.lock').unlink()
        with self.assertRaisesRegex(ValueError,'missing'):archive.load_generation(self.root)
        self.assertFalse((self.root/'owner.lock').exists())

    def test_redirected_and_escaping_trace_paths_are_refused(self):
        name=self.fixture.add();p=self.root/name;outside=Path(self.temp.name)/'outside.json';p.rename(outside);p.symlink_to(outside)
        with self.assertRaisesRegex(ValueError,'redirected'):archive.load_generation(self.root)
        p.unlink()
        self.fixture.events[-1]['result']['trace']='../outside.json';self.fixture.events[-2]['trace']='../outside.json';self.fixture.save()
        with self.assertRaisesRegex(ValueError,'escapes'):archive.load_generation(self.root)

    def test_changed_during_read_and_changed_terminal_manifest_are_refused(self):
        original=archive.Archive.verify_stable
        def raced(reader):
            path=self.root/'queue.json';path.write_bytes(path.read_bytes()+b' ');original(reader)
        with mock.patch.object(archive.Archive,'verify_stable',raced),self.assertRaisesRegex(ValueError,'changed'):
            archive.load_generation(self.root)
        self.fixture.rebind('queue.json')
        write(self.root/'manifest.json',{'complete':False})
        self.fixture.emit('session_finished',manifest_sha256=digest(self.root/'manifest.json'),complete=False)
        self.assertEqual(archive.load_generation(self.root)['manifest'],{'complete':False})
        write(self.root/'manifest.json',{'complete':True})
        with self.assertRaisesRegex(ValueError,'changed'):archive.load_generation(self.root)


if __name__=='__main__':unittest.main(verbosity=2)
