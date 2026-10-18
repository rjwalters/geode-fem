"""Controlled real child processes; fake numerical outputs are never scientific evidence."""
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import signal
import sys
import tempfile
import time
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
COMMIT = '1c6784e7e1fc8fe0564b1d793a6c4061588028f5'
sys.path.insert(0, str(HERE))
import refinement_run as run

CHILD = r'''
import json,sys,os,time
from pathlib import Path
args=dict(zip(sys.argv[1::2],sys.argv[2::2]))
if os.environ.get('REFINEMENT_TEST_SLOW')=='1':time.sleep(30)
if '--space' in args:
    request=json.loads(sys.stdin.readline())
    print(json.dumps(dict(op='ready',ok=True)))
    print(json.dumps(dict(request,ok=True)))
else:
    out=Path(args['--out']);out.mkdir()
    raw=Path(args['--request']).read_bytes();request=json.loads(raw)
    (out/'request.json').write_bytes(raw)
    if request['check']=='missing':sys.exit(3)
    (out/'result.json').write_text(json.dumps(dict(ok=request['check']=='primary',fixture=True)))
    sys.exit(0 if request['check']=='primary' else 2)
'''

DRIVER = r'''
from pathlib import Path
import json,sys
from unittest import mock
root=Path(sys.argv[1]);sys.path.insert(0,sys.argv[2])
import refinement_run as run
config=json.loads((root/'config.json').read_text())
with mock.patch.object(run.refinement_environment,'verify',return_value=config['environment']), mock.patch.object(run.refinement_bundle,'verify',return_value=config['bundle']):
 receipt=run.run(environment_path=root/'environment.json',environment_sha256=config['environment_sha256'],out=root/'execution',**config['arguments'])
(root/'receipt.json').write_text(json.dumps(receipt))
'''


class RefinementRunTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(); cls.root = Path(cls.temp.name).resolve()
        root = cls.root; bundle_root = root / 'bundle'; bundle_root.mkdir()
        (bundle_root / 'inputs').mkdir(); (bundle_root / 'inputs/L2.msh').write_text('fixture')
        bundle = {'geometry': [], 'jobs': []}
        for key in ('a', 'b'):
            request = {'id': 1, 'op': 'geometry', 'x_bits': [1, 2]}
            (bundle_root / (key+'.json')).write_text(json.dumps(request))
            bundle['geometry'].append(dict(design_key=key,space=key,request_file=key+'.json'))
        for check in ('primary', 'refused', 'missing'):
            (bundle_root / (check+'.json')).write_text(json.dumps({'check': check}))
            bundle['jobs'].append(dict(design_key='a',job_id='a/'+check,space='a',
                request_file=check+'.json',mesh_file='inputs/L2.msh',output_directory='results/a/'+check))
        build = root / 'build'; build.mkdir(); executables = {}
        for name in run.refinement_build.EXAMPLES:
            (build / name).mkdir(); path = build / name / name
            path.write_text('#!'+sys.executable+'\n'+CHILD);path.chmod(0o755)
            executables[name] = {'binary_sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
        env = {'bindings': {'build': {'executables': executables}}}
        environment = root / 'environment.json'; environment.write_text(json.dumps(env))
        args = dict(repo=str(REPO),bundle_root=str(bundle_root),bundle_sha256='b'*64,
                    build_root=str(build),build_sha256='c'*64,executor_commit=COMMIT)
        cls.config = dict(arguments=args,environment=env,bundle=bundle,
                          environment_sha256=hashlib.sha256(environment.read_bytes()).hexdigest())
        (root / 'config.json').write_text(json.dumps(cls.config));(root / 'driver.py').write_text(DRIVER)
        result = subprocess.run([sys.executable,'-I',str(root/'driver.py'),str(root),str(HERE)],
                                capture_output=True,text=True,timeout=30)
        if result.returncode:
            raise AssertionError(result.stderr)
        cls.receipt = json.loads((root/'receipt.json').read_text())
        cls.manifest = json.loads((root/'execution/execution.json').read_text())
        cls.events = [json.loads(line) for line in (root/'execution/events.jsonl').read_text().splitlines()]

    @classmethod
    def tearDownClass(cls): cls.temp.cleanup()

    def test_complete_schedule_keeps_refusal_and_missing_output_without_retry(self):
        rows = self.manifest['rows']
        self.assertEqual(len(rows),5)
        self.assertEqual([r['returncode'] for r in rows],[0,0,0,2,3])
        self.assertEqual(rows[-1]['status'],'unavailable')
        self.assertTrue((self.root/'execution/results/a/refused/native/result.json').is_file())
        self.assertEqual(len([e for e in self.events if e['kind']=='process_spawn_intent']),5)
        self.assertFalse(self.receipt['can_report'])
        run._verify_rows(self.root/'execution',self.manifest,self.config['bundle'],self.events)

    def test_owned_pid_is_preserved_across_exec_and_exit_receipts_are_retained(self):
        starts=[e['identity'] for e in self.events if e['kind']=='process_spawn_started']
        receipts=[e['receipt'] for e in self.events if e['kind']=='process_spawn_reaped']
        self.assertEqual(len(starts),5)
        for start, receipt in zip(starts,receipts):
            self.assertEqual(start['pid'],receipt['pid']);self.assertEqual(start['pid'],start['pgid'])
            self.assertEqual(receipt['state'],'reaped');self.assertFalse(receipt['forced_kill'])

    def test_separate_finalization_checks_real_stopped_fixture_processes(self):
        out=self.root/'finalized.json'
        with (mock.patch.object(run.refinement_environment,'verify',return_value=self.config['environment']),
              mock.patch.object(run.refinement_bundle,'verify',return_value=self.config['bundle'])):
            receipt=run.finalize(self.root/'execution',sha256=self.receipt['sha256'],
                environment_path=self.root/'environment.json',environment_sha256=self.config['environment_sha256'],
                out=out,**self.config['arguments'])
        self.assertTrue(receipt['can_report'])
        proof=json.loads(out.read_text());self.assertTrue(proof['processes']['report']['processes_stopped'])

    def test_missing_schedule_row_and_missing_terminal_journal_refused(self):
        changed=copy.deepcopy(self.manifest);changed['rows'].pop()
        with self.assertRaisesRegex(ValueError,'complete frozen schedule'):
            run._verify_rows(self.root/'execution',changed,self.config['bundle'],self.events)
        events=[e for e in self.events if e['kind']!='refinement_execution_completed']
        with self.assertRaisesRegex(ValueError,'incomplete request/child journal'):
            run._verify_rows(self.root/'execution',self.manifest,self.config['bundle'],events)

    def test_changed_exit_receipt_refused(self):
        events=copy.deepcopy(self.events)
        next(e for e in events if e['kind']=='owned_process_reaped')['receipt']['returncode']=9
        with self.assertRaisesRegex(ValueError,'journaled process outcome'):
            run._verify_rows(self.root/'execution',self.manifest,self.config['bundle'],events)

    def test_changed_evidence_digest_and_extra_output_refused(self):
        with self.assertRaisesRegex(ValueError,'retained receipt'):
            run._manifest(self.root/'execution','f'*64)
        extra=self.root/'execution/unrecorded';extra.write_text('extra')
        try:
            with self.assertRaisesRegex(ValueError,'census changed'):
                run._manifest(self.root/'execution',self.receipt['sha256'])
        finally:extra.unlink()

    def test_existing_run_directory_is_refused_before_environment_probe(self):
        with mock.patch.object(run.refinement_environment,'verify') as probe:
            with self.assertRaisesRegex(ValueError,'no implicit retries'):
                run.run(environment_path=self.root/'environment.json',
                    environment_sha256=self.config['environment_sha256'],out=self.root/'execution',
                    **self.config['arguments'])
            probe.assert_not_called()

    def test_stdin_adapter_rejects_changed_binary_before_launch(self):
        root=self.root/'changed-binary';root.mkdir();(root/'stdin').write_bytes(b'')
        binary=Path(self.config['arguments']['build_root'])/'icem_harness_eval/icem_harness_eval'
        result=subprocess.run([sys.executable,'-I',str(HERE/'refinement_exec.py'),str(root/'stdin'),
            str(root/'stdout'),str(root/'stderr'),str(binary),'f'*64],capture_output=True,text=True,timeout=10)
        self.assertNotEqual(result.returncode,0);self.assertIn('changed before exec',result.stderr)
        self.assertFalse((root/'stdout').exists())

    def test_wrong_geometry_query_identity_refused(self):
        root=self.root/'wrong-geometry';root.mkdir()
        (root/'stdout.jsonl').write_text(json.dumps(dict(op='ready',ok=True))+'\n'+
            json.dumps(dict(op='geometry',ok=True,id=2,x_bits=[1]))+'\n')
        with self.assertRaisesRegex(ValueError,'frozen query'):
            run._geometry(root,dict(id=1,x_bits=[1]))
        self.assertFalse((root/'geometry.json').exists())

    def test_interrupted_run_preserves_failure_and_reaps_owned_child(self):
        root=self.root/'interrupted';root.mkdir()
        (root/'config.json').write_text(json.dumps(self.config))
        (root/'environment.json').write_bytes((self.root/'environment.json').read_bytes())
        proc=subprocess.Popen([sys.executable,'-I',str(self.root/'driver.py'),str(root),str(HERE)],
            stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,
            env=dict(os.environ,REFINEMENT_TEST_SLOW='1'))
        try:
            deadline=time.monotonic()+10
            while time.monotonic()<deadline:
                path=root/'execution/events.jsonl'
                if path.exists() and 'owned_process_started' in path.read_text():break
                if proc.poll() is not None: self.fail(proc.communicate()[1])
                time.sleep(.02)
            else:self.fail('owned child did not start')
            proc.send_signal(signal.SIGTERM)
            _,stderr=proc.communicate(timeout=10)
            self.assertNotEqual(proc.returncode,0,stderr)
            self.assertTrue((root/'execution/execution-failed.json').is_file())
            self.assertFalse((root/'execution/execution.json').exists())
            with run.producer_source.committed_modules(REPO,COMMIT,run.MODULES):
                proof=run.importlib.import_module('process_verification').verify(root/'execution')
            self.assertTrue(proof['processes_stopped'],proof['issues'])
        finally:
            if proc.poll() is None:
                proc.terminate();proc.communicate(timeout=10)


if __name__ == '__main__': unittest.main(verbosity=2)
