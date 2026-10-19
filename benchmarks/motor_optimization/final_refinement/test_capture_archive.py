"""End-to-end capture/reader wiring with an explicitly synthetic 100-job study.

Real reviewed Git sources, production recovery policy and process verifier are
used. All optimizer/build/runtime observations are fixtures; no FEM, compilation,
main-study closure, or scientific eligibility is asserted.
"""
import copy
import hashlib
import importlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import capture_archive
import closed_capture
import producer_source
from test_trace_archive import Fixture,write,digest
from test_recovery_evidence import configure,finish

REPO=HERE.parents[2]
PRODUCER='9486a9da58beeca91968d4d3608f06b998abff45'
CAPTURE_TOOL='01350accb689c9dcb3a6add977e17f3d5e5be523'


def fixture(root):
    repo=root/'producer';repo.mkdir()
    reference=repo/'benchmarks/motor_reference/reference_data.json';reference.parent.mkdir(parents=True)
    reference.write_bytes((REPO/'benchmarks/motor_reference/reference_data.json').read_bytes())
    files=producer_source.committed_files(REPO,PRODUCER)
    source={'repo':str(repo),'commit':PRODUCER,'tree_clean':True,'untracked_sources':[],'files':files}
    build=root/'build';build.mkdir();(build/'owner.lock').touch()
    binary=build/'icem_harness_eval';binary.write_bytes(b'synthetic unexecuted evaluator fixture');binary.chmod(0o555)
    setup={'source':source,'target_directory':str(repo/'target')}
    write(build/'inputs.json',setup)
    artifact={'reason':'compiler-artifact','target':{'name':'icem_harness_eval','kind':['example']},
              'executable':str(repo/'target/release/examples/icem_harness_eval')}
    (build/'cargo.log').write_text(json.dumps(artifact)+'\n'+json.dumps({'reason':'build-finished','success':True})+'\n')
    receipt={'schema':1,'status':'recorded','source':source,
             'inputs':{'file':'inputs.json','sha256':digest(build/'inputs.json')},
             'log':{'file':'cargo.log','sha256':digest(build/'cargo.log')},
             'binary':{'file':'icem_harness_eval','sha256':digest(binary)},'cargo_artifact':artifact,
             'launcher_receipt':{'returncode':0,'forced_kill':False}}
    write(build/'build-receipt.json',receipt)
    f=Fixture(root/'main');configure(f)
    f.bindings['session_directory']=str(f.root);f.bindings['recovery_registry']=str(f.root)+'.recoveries'
    f.session.update(procs=15,evaluator={'binary':str(binary),'build_receipt':str(build/'build-receipt.json')})
    write(f.root/'session.json',f.session);f.rebind('session.json')
    for i in range(100):f.add(i,validations=False)
    frozen={'schema':1,'source':source,'runtime':{'fixture':'not a measured installed runtime'},
            'host':{'fixture':True},'session':f.session,'procs':15,'unit_seconds':2.,
            'inputs':{'binary':{'path':str(binary),'sha256':digest(binary)},
                      'build_receipt':{'path':str(build/'build-receipt.json'),'sha256':digest(build/'build-receipt.json')}}}
    write(f.root/'recovery-inputs.json',frozen);f.rebind('recovery-inputs.json')
    verified={'reviewed_commit':PRODUCER,'files':files}
    with producer_source.committed_modules(REPO,PRODUCER,capture_archive.PROCESS_MODULES):
        identity=importlib.import_module('process_identity');verifier=importlib.import_module('process_verification')
        absent={'pid':2147483647,'pgid':2147483647,'start':'synthetic absent owner','host':identity.host()}
        f.events[0].update(process_tracking_schema=1,owner_identity=absent);f.save();finish(f,True)
        events=[dict(sequence=0,kind='session_created',process_tracking_schema=1,owner_identity=absent),
                dict(sequence=1,kind='build_recorded',receipt={'file':'build-receipt.json','sha256':digest(build/'build-receipt.json')})]
        (build/'events.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in events))
        proof={'verified':True,'path':str(build/'build-receipt.json'),'sha256':digest(build/'build-receipt.json'),
               'binary_sha256':digest(binary),'source_commit':PRODUCER}
        bindings={'observed':frozen,'build_verification':proof,
                  'binding_verification':{'verified':True,'schema':1,'can_launch':False}}
        result=closed_capture._export(f.root,repo,root/'capture',frozen,verified,
              chain_probe=lambda source,registry:closed_capture._chain(source,registry,REPO,verified),
              binding_probe=lambda:copy.deepcopy(bindings),
              process_probe=lambda chain,build:closed_capture._processes(chain,build,verifier))
    return result


class CaptureArchiveTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp=tempfile.TemporaryDirectory();cls.base=Path(cls.temp.name).resolve()
        cls.result=fixture(cls.base)

    @classmethod
    def tearDownClass(cls):cls.temp.cleanup()

    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name).resolve()/'capture'
        shutil.copytree(self.base/'capture',self.root)
        self.manifest_sha256=self.result['sha256']

    def load(self,**kwargs):
        args=dict(manifest_sha256=self.manifest_sha256,git_repo=REPO,reviewed_commit=PRODUCER,capture_tool_commit=CAPTURE_TOOL)
        args.update(kwargs);return capture_archive.load(self.root,**args)

    def repin(self):
        # Test-only new witness digest: reach semantic checks after an intentional
        # mutation. Real callers must use the independently observed capture hash.
        path=self.root/'capture.json';m=json.loads(path.read_text())
        inventory=closed_capture.census(self.root);inventory['files'].pop('capture.json')
        m['inventory']=inventory;write(path,m);self.manifest_sha256=digest(path)

    def alter(self,name,mutate):
        path=self.root/name;x=json.loads(path.read_text());mutate(x);write(path,x);self.repin()

    def test_full_synthetic_capture_replays_policy_build_sources_and_processes_read_only(self):
        before={str(p):(p.read_bytes(),p.stat().st_mtime_ns) for p in self.root.rglob('*') if p.is_file()}
        result=self.load()
        self.assertTrue(result['capture_verified']);self.assertFalse(result['can_select']);self.assertFalse(result['can_launch'])
        self.assertTrue(result['chain']['all_registered_jobs_resolved'])
        self.assertEqual(len(result['chain']['policy']['jobs']),100)
        self.assertTrue(result['build_verification']['verified'])
        self.assertEqual(len(result['source_verification']['files']),1419)
        for path,(raw,mtime) in before.items():
            self.assertEqual(Path(path).read_bytes(),raw);self.assertEqual(Path(path).stat().st_mtime_ns,mtime)

    def test_manifest_trust_anchor_and_complete_census_are_required(self):
        with self.assertRaisesRegex(ValueError,'independently recorded'):self.load(manifest_sha256='a'*64)
        extra=self.root/'extra.json';extra.write_text('{}')
        with self.assertRaisesRegex(ValueError,'census'):self.load()
        extra.unlink();(self.root/'study/main/owner.lock').unlink()
        with self.assertRaisesRegex(ValueError,'census'):self.load()

    def test_captured_code_or_source_declaration_cannot_choose_its_trust_anchor(self):
        (self.root/'capture-tool/closed_capture.py').write_text('raise RuntimeError("must never execute")\n')
        self.repin()
        with self.assertRaises(ValueError):self.load()
        with self.assertRaises(ValueError):self.load(reviewed_commit='b'*40)
        with self.assertRaises(ValueError):self.load(capture_tool_commit='HEAD')

    def test_altered_producer_observation_is_not_a_verified_runtime_binding(self):
        self.alter('producer-after.json',lambda r:r['observed'].update(runtime={'changed':True}))
        with self.assertRaisesRegex(ValueError,'producer observations'):self.load()

    def test_build_binary_and_successful_cargo_artifact_must_match(self):
        (self.root/'build/icem_harness_eval').chmod(0o644)
        (self.root/'build/icem_harness_eval').write_bytes(b'changed binary');self.repin()
        with self.assertRaises(ValueError):self.load()

    def test_changed_process_table_cannot_claim_a_live_owner_is_absent(self):
        def mutate(e):
            report=next(iter(e['reports'].values()));owner=report['observations'][0]['recorded']
            e['table'].append({'pid':owner['pid'],'ppid':1,'pgid':owner['pgid'],'start':owner['start'],'state':'S'})
        self.alter('processes-after.json',mutate)
        with self.assertRaisesRegex(ValueError,'process absence'):self.load()

    def test_process_report_requires_all_journals_and_recomputable_observations(self):
        self.alter('processes-before.json',lambda e:next(iter(e['reports'].values()))['journals']['.'].update(sha256='a'*64))
        with self.assertRaisesRegex(ValueError,'verifier replay'):self.load()

    def test_process_timestamp_must_belong_to_capture(self):
        self.alter('processes-before.json',lambda e:next(iter(e['reports'].values())).update(observed_time=0.))
        with self.assertRaisesRegex(ValueError,'identity/time'):self.load()

    def test_changed_reconciled_policy_is_refused(self):
        self.alter('reconciled-policy.json',lambda p:p.update(unresolved_job_count=1))
        with self.assertRaisesRegex(ValueError,'recovery history'):self.load()

    def test_input_path_escape_and_redirected_file_are_refused(self):
        self.alter('capture.json',lambda m:m['inputs']['reference_data'].update(file='../outside'))
        with self.assertRaisesRegex(ValueError,'relative path'):self.load()

    def test_recorded_source_map_cannot_replace_reviewed_git_content(self):
        self.alter('study/main/recovery-inputs.json',lambda f:f['source']['files'].update({'Cargo.toml':'a'*64}))
        with self.assertRaisesRegex(ValueError,'reviewed Git'):self.load()

    def test_process_capture_cannot_omit_a_generation(self):
        self.alter('processes-before.json',lambda e:e['reports'].pop(next(iter(e['reports']))))
        with self.assertRaisesRegex(ValueError,'omits a generation'):self.load()

    def test_recomputed_policy_detects_a_missing_hundredth_completion(self):
        path=self.root/'study/main/events.jsonl'
        events=[json.loads(line) for line in path.read_text().splitlines()]
        last=max(i for i,e in enumerate(events) if e['kind']=='attempt_finished')
        events.pop(last)
        for i,e in enumerate(events):e['sequence']=i
        path.write_text(''.join(json.dumps(e)+'\n' for e in events));self.repin()
        with self.assertRaisesRegex(ValueError,'recovery history'):self.load()

    def test_canonical_mapping_cannot_alias_another_tree(self):
        self.alter('capture.json',lambda m:m['directories'].update({m['canonical']['source']:'build'}))
        with self.assertRaisesRegex(ValueError,'mapping'):self.load()


if __name__=='__main__':unittest.main(verbosity=2)
