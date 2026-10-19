"""Execution prerequisite failures, using controlled runtime observations."""
import copy
from contextlib import nullcontext
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import refinement_environment as environment


class RefinementEnvironmentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        for name in ('repo', 'bundle', 'build'):
            (self.root / name).mkdir()
        self.args = dict(repo=self.root / 'repo', bundle_root=self.root / 'bundle',
                         build_root=self.root / 'build', bundle_sha256='b'*64,
                         build_sha256='c'*64, executor_commit='a'*40)
        self.bound = {'source': 'controlled fixture'}
        self.env = {'PATH': '/fixture', 'RAYON_NUM_THREADS': '1',
                    'GEODE_NUM_THREADS': '1', 'UV_OFFLINE': '1'}
        self.runtime = {'environment_sha256': environment._hash(self.env), 'mesher': 'fixture'}
        self.processes = {'host': {'boot': 'fixture'}, 'reports': {'processes_stopped': True}}
        previous = Path.cwd()
        os.chdir(self.args['repo'])
        self.addCleanup(os.chdir, previous)

    def capture(self, *, bindings=None, runtime=None, env=None):
        with (mock.patch.object(environment, '_bindings', side_effect=bindings or [self.bound, self.bound]),
              mock.patch.object(environment, 'execution_env', side_effect=env or [self.env, self.env]),
              mock.patch.object(environment.producer_source, 'committed_modules', return_value=nullcontext()),
              mock.patch.object(environment.importlib, 'import_module', return_value=None),
              mock.patch.object(environment, '_runtime', return_value=runtime or self.runtime),
              mock.patch.object(environment, '_processes', return_value=self.processes)):
            return environment.capture(**self.args)

    def test_controls_are_explicit_and_parent_environment_is_unchanged(self):
        with mock.patch.dict(os.environ, {'GEODE_NUM_THREADS': '16', 'UV_OFFLINE': '0'}, clear=True):
            self.assertEqual(environment.execution_env(), {
                'GEODE_NUM_THREADS': '1', 'UV_OFFLINE': '1', 'RAYON_NUM_THREADS': '1'})
            self.assertEqual(os.environ['GEODE_NUM_THREADS'], '16')

    def test_capture_retains_evidence_without_granting_execution(self):
        result = self.capture()
        self.assertEqual(result['bindings'], self.bound)
        self.assertEqual(result['runtime'], self.runtime)
        self.assertEqual(result['processes'], self.processes)
        self.assertFalse(result['can_launch'])

    def test_changed_inputs_after_runtime_probe_are_refused(self):
        with self.assertRaisesRegex(ValueError, 'inputs or executors changed'):
            self.capture(bindings=[self.bound, {'source': 'different'}])

    def test_changed_environment_during_probe_is_refused(self):
        with self.assertRaisesRegex(ValueError, 'environment changed'):
            self.capture(env=[self.env, dict(self.env, PATH='/different')])

    def test_original_executor_working_directory_is_required_before_probes(self):
        os.chdir(self.root)
        with mock.patch.object(environment, '_bindings') as probe:
            with self.assertRaisesRegex(ValueError, 'pinned executor worktree'):
                environment.capture(**self.args)
            probe.assert_not_called()

    def test_runtime_rejects_wrong_pinned_version_and_requires_offline_env(self):
        uv = self.root / 'uv'; uv.write_bytes(b'fixture')
        mesher = {'isolated': True, 'distributions': {'numpy': {'version': 'wrong'},
                                                   'gmsh': {'version': '4.15.2'}}}
        with (mock.patch.object(environment.shutil, 'which', return_value=str(uv)),
              mock.patch.object(environment.subprocess, 'run', return_value=SimpleNamespace(
                  returncode=0, stdout=json.dumps(mesher))) as run):
            with self.assertRaisesRegex(ValueError, 'pinned package versions'):
                environment._runtime(self.args['repo'], {'runtime_fingerprint': None}, self.env)
            self.assertEqual(run.call_args.kwargs['env']['UV_OFFLINE'], '1')
            self.assertIn('-I', run.call_args.args[0])

    def test_live_build_process_refused(self):
        identity = SimpleNamespace(table=lambda: [], host=lambda: {'boot': 'fixture'})
        verifier = SimpleNamespace(verify=lambda *a, **k: {'processes_stopped': False})
        with self.assertRaisesRegex(ValueError, 'not verifiably stopped'):
            environment._processes(self.args['build_root'], {'process_identity': identity,
                                                        'process_verification': verifier})

    def test_bindings_refuse_reference_and_construction_source_changes(self):
        reference = self.args['repo'] / 'benchmarks/motor_reference/reference_data.json'
        reference.parent.mkdir(parents=True); reference.write_bytes(b'reference')
        constructor = self.args['repo'] / 'constructor.py'; constructor.write_bytes(b'geometry')
        bundle = {'reference_sha256': environment.producer_source.digest(reference),
                  'mesh_evidence': {'construction_source_sha256': {
                      'constructor.py': environment.producer_source.digest(constructor)}},
                  'capture_manifest_sha256': 'd'*64, 'reviewed_producer_commit': 'e'*40}
        build = {'source': {'commit': 'a'*40}, 'executables': {}}
        def bind():
            return environment._bindings(self.args['bundle_root'], 'b'*64,
                self.args['build_root'], 'c'*64, self.args['repo'], 'a'*40)
        with (mock.patch.object(environment.refinement_bundle, 'verify', return_value=bundle),
              mock.patch.object(environment.refinement_build, 'verify', return_value=build)):
            bind(); constructor.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError, 'construction source'): bind()
            reference.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError, 'reference-data path'): bind()

    def test_record_never_writes_inside_verified_inputs_or_overwrites(self):
        with mock.patch.object(environment, 'capture') as probe:
            for root in ('repo', 'bundle_root', 'build_root'):
                with self.assertRaisesRegex(ValueError, 'outside verified'):
                    environment.record(out=self.args[root] / 'new.json', **self.args)
            path = self.root / 'evidence.json'; path.write_text('{}')
            with self.assertRaisesRegex(ValueError, 'must be new'):
                environment.record(out=path, **self.args)
            probe.assert_not_called()

    def test_verify_uses_fresh_process_evidence_and_refuses_runtime_or_boot_drift(self):
        evidence = self.capture()
        path = self.root / 'evidence.json'
        with mock.patch.object(environment, 'capture', return_value=evidence):
            receipt = environment.record(out=path, **self.args)
        fresh = copy.deepcopy(evidence); fresh['processes']['fresh'] = True
        with mock.patch.object(environment, 'capture', return_value=fresh) as probe:
            self.assertTrue(environment.verify(path, sha256=receipt['sha256'], **self.args)['processes']['fresh'])
            probe.assert_called_once()
            fresh['runtime']['mesher'] = 'changed'
            with self.assertRaisesRegex(ValueError, 'runtime changed'):
                environment.verify(path, sha256=receipt['sha256'], **self.args)
            fresh['runtime'] = evidence['runtime']; fresh['processes']['host'] = {'boot': 'new'}
            with self.assertRaisesRegex(ValueError, 'host or boot changed'):
                environment.verify(path, sha256=receipt['sha256'], **self.args)
        path.write_text('{}')
        with mock.patch.object(environment, 'capture') as probe:
            with self.assertRaisesRegex(ValueError, 'retained receipt'):
                environment.verify(path, sha256=receipt['sha256'], **self.args)
            probe.assert_not_called()


if __name__ == '__main__':
    unittest.main(verbosity=2)
