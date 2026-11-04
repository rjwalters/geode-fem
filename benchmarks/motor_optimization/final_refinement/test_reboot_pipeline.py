"""Synthetic full scientific pipeline; no genuine main-study records or execution."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE=Path(__file__).resolve().parent;sys.path.insert(0,str(HERE))
import closed_capture
import refinement_bundle
import study_selection
from mesh_test_support import CACHE
from test_reboot_attestation import B0, raw
from test_reboot_capture import committed_test_tool, exported_fixture, PRODUCER
import test_study_selection as scientific_fixture
from test_refinement_bundle import MESH_COMMIT


class RebootPipelineTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        scientific_fixture.StudySelectionTests.setUpClass()
        cls.fixture=scientific_fixture.StudySelectionTests('test_public_capture_setup_validation_selection_chain')
        cls.fixture.setUp()
        prepared=cls.fixture.scientific_capture(build_name='build-record',export=False,
            owner_host={'host':'fixture-host','platform':'linux','boot':B0})
        # Production entry points must import their own pinned ledger module.
        scientific_fixture.StudySelectionTests.module_context.__exit__(None,None,None)
        cls.base=Path(cls.fixture.temp.name).resolve()/'reboot-pipeline';cls.base.mkdir()
        cls.repo,cls.commit=committed_test_tool(cls.base)
        cls.receipt,cls.policy_sha=exported_fixture(cls.base,prepared=prepared)
        cls.capture=cls.base/'capture'
        cls.args={'manifest_sha256':cls.receipt['sha256'],'git_repo':cls.repo,
                  'reviewed_commit':PRODUCER,'capture_tool_commit':cls.commit,
                  'reboot_policy_sha256':cls.policy_sha}
        cls.bundle_args={**cls.args,'mesh_evidence_commit':MESH_COMMIT,
                         'l3_mesh':CACHE/'614bf98c36e105adca2b6f811f09ebe4aa1c4d1c0a456eb8c3041bf81bdd584a'}

    @classmethod
    def tearDownClass(cls):
        cls.fixture.doCleanups()
        scientific_fixture.StudySelectionTests.b.tearDownClass()

    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.out=Path(self.temp.name).resolve()

    def test_public_selection_and_bundle_preserve_reboot_proof_scope(self):
        before=closed_capture.census(self.capture)
        result=study_selection.load(self.capture,**self.args)
        self.assertEqual(len(result['attempts']),100);self.assertEqual(len(result['plan']['jobs']),21)
        self.assertEqual(result['reboot_verification']['policy_sha256'],self.policy_sha)
        self.assertFalse(result['can_launch'])
        receipt=refinement_bundle.prepare(self.capture,out=self.out/'bundle',**self.bundle_args)
        metadata=refinement_bundle.verify(self.out/'bundle',manifest_sha256=receipt['sha256'])
        self.assertEqual(metadata['reboot_verification'],result['reboot_verification'])
        self.assertEqual(len(metadata['jobs']),21);self.assertFalse(metadata['can_launch'])
        self.assertEqual(closed_capture.census(self.capture),before)

    def test_both_public_entry_points_require_external_policy_before_output(self):
        for digest in (None,'e'*64):
            with self.subTest(digest=digest):
                with self.assertRaisesRegex(ValueError,'independently supplied'):
                    study_selection.load(self.capture,**dict(self.args,reboot_policy_sha256=digest))
                with self.assertRaisesRegex(ValueError,'independently supplied'):
                    refinement_bundle.prepare(self.capture,out=self.out/'missing',
                        **dict(self.bundle_args,reboot_policy_sha256=digest))
                self.assertFalse((self.out/'missing').exists())

    def test_selection_and_bundle_cli_accept_external_policy(self):
        for name,args,output in [('study_selection.py',self.args,self.out/'selection.json'),
                                 ('refinement_bundle.py',self.bundle_args,self.out/'cli-bundle')]:
            with self.subTest(name=name):
                command=[sys.executable,'-I',str(HERE/name),'--capture',str(self.capture),'--out',str(output)]
                for key,value in args.items():command.extend(['--'+key.replace('_','-'),str(value)])
                result=subprocess.run(command,capture_output=True,text=True)
                self.assertEqual(result.returncode,0,result.stderr)
                if name=='study_selection.py':
                    metadata=json.loads(output.read_bytes())
                else:
                    receipt=json.loads(result.stdout)
                    metadata=refinement_bundle.verify(output,manifest_sha256=receipt['sha256'])
                self.assertEqual(metadata['reboot_verification']['policy_sha256'],self.policy_sha)

    def test_bundle_cannot_discard_selection_reboot_scope_after_fixture_repin(self):
        root=self.out/'bundle';refinement_bundle.prepare(self.capture,out=root,**self.bundle_args)
        path=root/'bundle.json';metadata=json.loads(path.read_bytes());metadata.pop('reboot_verification')
        path.write_bytes(raw(metadata))
        import hashlib
        with self.assertRaisesRegex(ValueError,'reboot verification scope'):
            refinement_bundle.verify(root,manifest_sha256=hashlib.sha256(path.read_bytes()).hexdigest())


if __name__=='__main__':unittest.main()
