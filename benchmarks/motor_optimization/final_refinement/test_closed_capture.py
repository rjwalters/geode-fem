"""Snapshot transaction tests; injected fixtures are not live-study provenance."""
import copy
import importlib
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
from unittest import mock
import unittest

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import closed_capture as capture
import producer_source
from test_trace_archive import Fixture
from test_recovery_evidence import finish,configure

REVIEWED='9486a9da58beeca91968d4d3608f06b998abff45'
REPO=HERE.parents[2]


class ClosedCaptureTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name).resolve();self.repo=self.root/'repo';self.repo.mkdir()
        ref=self.repo/'benchmarks/motor_reference/reference_data.json';ref.parent.mkdir(parents=True);ref.write_text('{}\n')
        self.source=self.root/'main';self.source.mkdir();(self.source/'owner.lock').write_text('')
        (self.source/'events.jsonl').write_text('{}\n')
        (self.source/'meshes').mkdir();(self.source/'meshes/candidate.msh').write_bytes(b'original mesh fixture')
        self.build=self.root/'build';self.build.mkdir();(self.build/'build-receipt.json').write_text('{}\n')
        binary=self.build/'icem_harness_eval';binary.write_bytes(b'not a real executable');binary.chmod(0o555)
        self.expected={'session':{'evaluator':{'build_receipt':str(self.build/'build-receipt.json')}},
                       'inputs':{'binary':{'path':str(binary),'sha256':producer_source.digest(binary)}}}
        self.verified={'reviewed_commit':'a'*40,'files':{'benchmarks/motor_reference/reference_data.json':producer_source.digest(ref)}}
        self.chain={'source_directories':[str(self.source)],'policy':{'fixture':'synthetic complete policy'}}
        self.bindings={'fixture':'synthetic binding result'}
        self.processes={'host':{'host':'synthetic'},'table':[],
                        'reports':{str(self.source):{'processes_stopped':True},str(self.build):{'processes_stopped':True}}}
        self.out=self.root/'capture'

    def export(self,**kwargs):
        args=dict(chain_probe=lambda *args:copy.deepcopy(self.chain),
                  binding_probe=lambda:copy.deepcopy(self.bindings),
                  process_probe=lambda *args:copy.deepcopy(self.processes))
        args.update(kwargs)
        return capture._export(self.source,self.repo,self.out,self.expected,self.verified,**args)

    def test_copies_complete_trees_inputs_tool_and_observations_without_mutation(self):
        before={str(p):(p.read_bytes(),p.stat().st_mtime_ns) for r in (self.source,self.build,self.repo)
                for p in r.rglob('*') if p.is_file()}
        result=self.export();manifest=json.loads(Path(result['manifest']).read_text())
        self.assertFalse(result['can_select']);self.assertFalse(result['can_launch'])
        self.assertEqual(result['sha256'],producer_source.digest(self.out/'capture.json'))
        self.assertEqual(manifest['canonical']['source'],str(self.source))
        self.assertIsNone(manifest['registry_directory'])
        self.assertEqual((self.out/'study/main/meshes/candidate.msh').read_bytes(),b'original mesh fixture')
        for name,item in manifest['inputs'].items():self.assertEqual(producer_source.digest(self.out/item['file']),item['sha256'])
        for path,(raw,mtime) in before.items():
            self.assertEqual(Path(path).read_bytes(),raw);self.assertEqual(Path(path).stat().st_mtime_ns,mtime)
        self.assertIn('capture-tool/producer_source.py',manifest['inventory']['files'])
        self.assertFalse((self.source.parent/'main.recoveries').exists())

    def test_existing_output_and_output_inside_sources_are_refused(self):
        self.out.mkdir()
        with self.assertRaisesRegex(ValueError,'already exists'):self.export()
        self.out=self.source/'capture'
        with self.assertRaisesRegex(ValueError,'outside'):self.export()

    def test_symlinks_special_files_and_wrong_input_hash_are_refused(self):
        alias=self.source/'alias';alias.symlink_to(self.build/'icem_harness_eval')
        with self.assertRaisesRegex(ValueError,'redirected'):self.export()
        alias.unlink();os.mkfifo(alias)
        with self.assertRaisesRegex(ValueError,'special'):self.export()
        alias.unlink();self.expected['inputs']['binary']['sha256']='b'*64
        with self.assertRaisesRegex(ValueError,'input changed'):self.export()
        self.assertFalse(self.out.exists())

    def test_changed_files_or_new_registry_leave_only_failed_capture(self):
        calls=0
        def binding():
            nonlocal calls
            calls+=1
            if calls==2:(self.source/'new-output.json').write_text('{}')
            return copy.deepcopy(self.bindings)
        with self.assertRaisesRegex(ValueError,'census changed'):self.export(binding_probe=binding)
        self.assertFalse((self.out/'capture.json').exists())
        self.assertTrue((self.out/'capture-failed.json').exists())
        self.out=self.root/'second';calls=0
        def binding2():
            nonlocal calls
            calls+=1
            if calls==2:self.source.with_name('main.recoveries').mkdir()
            return copy.deepcopy(self.bindings)
        with self.assertRaisesRegex(ValueError,'registry changed'):self.export(binding_probe=binding2)
        self.assertFalse((self.out/'capture.json').exists())

    def test_binding_or_process_host_changes_are_refused(self):
        calls=0
        def binding():
            nonlocal calls
            calls+=1;return {'runtime':calls}
        with self.assertRaisesRegex(ValueError,'changed during capture'):self.export(binding_probe=binding)
        self.out=self.root/'second';calls=0
        def process(*args):
            nonlocal calls
            calls+=1;result=copy.deepcopy(self.processes);result['host']['boot']=str(calls);return result
        with self.assertRaisesRegex(ValueError,'changed during capture'):self.export(process_probe=process)

    def test_registered_recovery_tree_is_copied_with_canonical_mapping(self):
        registry=self.source.with_name('main.recoveries');registry.mkdir()
        (registry/'generation-0001').mkdir();(registry/'generation-0001/evidence.json').write_text('{}')
        self.chain['source_directories'].append(str(registry/'generation-0001'))
        result=self.export();m=json.loads(Path(result['manifest']).read_text())
        self.assertEqual(m['registry_directory'],'study/main.recoveries')
        self.assertTrue((self.out/'study/main.recoveries/generation-0001/evidence.json').exists())

    def test_runtime_source_input_equality_and_build_verification_are_required(self):
        expected={'session':{'evaluator':{'build_receipt':'receipt','binary':'binary'}},
                  'source':{'commit':'reviewed'},'runtime':{'identity':'original'},
                  'inputs':{'binary':{'sha256':'a'*64}},'host':{'host':'original'},
                  'procs':15,'unit_seconds':70.}
        observed=copy.deepcopy(expected)
        bindings=SimpleNamespace(capture=mock.Mock(side_effect=lambda *a,**k:copy.deepcopy(observed)),
                                 verify=mock.Mock(return_value={'verified':True}))
        build=SimpleNamespace(verify=mock.Mock(return_value={'verified':True}))
        result=capture._bindings(expected,self.repo,bindings,build)
        self.assertEqual(result['observed'],expected)
        build.verify.assert_called_once_with('receipt','binary',expected['source'])
        for field in ('runtime','source','inputs','host','procs','unit_seconds'):
            observed=copy.deepcopy(expected);observed[field]='changed'
            with self.subTest(field=field),self.assertRaisesRegex(ValueError,'bindings changed'):
                capture._bindings(expected,self.repo,bindings,build)
        observed=copy.deepcopy(expected);build.verify.side_effect=ValueError('build differs')
        with self.assertRaisesRegex(ValueError,'build differs'):
            capture._bindings(expected,self.repo,bindings,build)

    def test_actual_recovery_policy_requires_all_100_closed_jobs_and_original_paths(self):
        f=Fixture(self.root/'original');configure(f)
        f.bindings['session_directory']=str(f.root);f.bindings['recovery_registry']=str(f.root)+'.recoveries'
        f.rebind('session.json')
        hashes={producer_source.HARNESS+n:producer_source.digest(REPO/producer_source.HARNESS/n)
                for n in ('recovery_policy.py','session_journal.py','process_identity.py')}
        for i in range(99):f.add(i,validations=False)
        finish(f,True)
        with self.assertRaisesRegex(ValueError,'not closed'):
            capture._chain(f.root,None,REPO,{'files':hashes})
        # Rebuild the terminal binding after the final synthetic scientific failure.
        f.events.pop();f.save();f.add(99,validations=False);finish(f,True)
        self.assertTrue(capture._chain(f.root,None,REPO,{'files':hashes})['all_registered_jobs_resolved'])
        f.bindings['session_directory']='/other/main';f.bindings['recovery_registry']='/other/main.recoveries'
        f.rebind('session.json')
        with self.assertRaisesRegex(ValueError,'original study paths'):
            capture._chain(f.root,None,REPO,{'files':hashes})

    def test_actual_process_verifier_observes_live_owner_and_absence(self):
        names=['process_identity','session_journal','recovery_inventory','process_verification']
        with producer_source.committed_modules(REPO,REVIEWED,names):
            identity=importlib.import_module('process_identity')
            verifier=importlib.import_module('process_verification')
            for root in (self.source,self.build):
                (root/'owner.lock').touch()
                event={'sequence':0,'kind':'session_created','process_tracking_schema':1,
                       'owner_identity':dict(pid=2147483647,pgid=2147483647,start='absent fixture',host=identity.host())}
                (root/'events.jsonl').write_text(json.dumps(event)+'\n')
            proof=capture._processes(self.chain,self.build,verifier)
            self.assertTrue(proof['table'])
            self.assertTrue(all(r['processes_stopped'] for r in proof['reports'].values()))
            event['owner_identity']=identity.identify(os.getpid())
            (self.source/'events.jsonl').write_text(json.dumps(event)+'\n')
            with self.assertRaisesRegex(ValueError,'not proven stopped'):
                capture._processes(self.chain,self.build,verifier)


if __name__=='__main__':unittest.main(verbosity=2)
