"""Replay real frozen setup records, without interpreting live optimization data."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

HERE=Path(__file__).resolve().parent;REPO=HERE.parents[2]
sys.path.insert(0,str(HERE))
import setup_evidence
import producer_source


class SetupEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp=tempfile.TemporaryDirectory();root=Path(cls.temp.name).resolve()
        archive=HERE/'smoke/live_frozen_setup_9486a9da.tar.gz'
        assert hashlib.sha256(archive.read_bytes()).hexdigest()=='d4bd51232f9ac3c4b7a4e3635f798a987391dabe6694193ae9acc6705303a58f'
        with tarfile.open(archive) as a:a.extractall(root,filter='data')
        inventory=json.loads((root/'manifest.json').read_text())
        for name,digest in inventory.items():assert hashlib.sha256((root/name).read_bytes()).hexdigest()==digest
        names=('bindings.json','session.json','start_preflight.json','anchor_preflight.json','calibration_primary.json','calibration_secondary.json')
        cls.original={n:json.loads((root/n).read_text()) for n in names}
        for name in names[1:]:
            assert hashlib.sha256((root/name).read_bytes()).hexdigest()==cls.original['bindings.json']['files'][name]
        cls.frozen=json.loads((root/'recovery-inputs.json').read_text())
        cls.commit=cls.frozen['source']['commit']
        cls.source=producer_source.verify(REPO,cls.frozen['source'],reviewed_commit=cls.commit,check_worktree=False)
        cls.inputs={'reference_data':REPO/'benchmarks/motor_reference/reference_data.json',
                    'starts':REPO/'benchmarks/motor_optimization/harness/starts.json',
                    'anchor':REPO/'benchmarks/motor_optimization/anchor.json'}

    @classmethod
    def tearDownClass(cls):cls.temp.cleanup()

    def setUp(self):self.records=copy.deepcopy(self.original)

    def check(self):
        return setup_evidence.check(self.records,frozen=self.frozen,inputs=self.inputs,source=self.source,
                                    git_repo=REPO,reviewed_commit=self.commit)

    def test_real_setup_preserves_canonical_anchor_and_primary_common_unit(self):
        result=self.check()
        self.assertEqual(result['unit_seconds'],70.935572288)
        self.assertEqual(result['effective_anchor']['F0'],.009603404147653405)
        self.assertEqual(set(result['theta0']),{'a','b'})
        self.assertNotEqual(result['theta0']['a']['design'].key,result['theta0']['b']['design'].key)
        self.assertFalse(result['can_select']);self.assertFalse(result['can_launch'])
        self.assertEqual(self.records,self.original)

    def test_session_anchor_receipt_and_training_path_are_bound(self):
        for kind in ('anchor', 'mesh'):
            self.records=copy.deepcopy(self.original)
            session=self.records['session.json']
            if kind=='anchor':session['anchor_recomputation']['sha256']='0'*64
            else:session['models']['L0']['train_mesh']='/wrong/mesh.msh'
            frozen=copy.deepcopy(self.frozen);frozen['session']=session
            with self.subTest(kind=kind),self.assertRaises(ValueError):
                setup_evidence.check(self.records,frozen=frozen,inputs=self.inputs,source=self.source,
                                     git_repo=REPO,reviewed_commit=self.commit)

    def test_recomputed_anchor_cannot_replace_canonical_targets(self):
        row=self.records['anchor_preflight.json']['sessions'][0]
        row['ready']['anchor']['F0']=row['reply']['validation']['F']
        with self.assertRaisesRegex(ValueError,'anchor parse'):self.check()

    def test_missing_space_or_changed_waveform_is_refused(self):
        self.records['anchor_preflight.json']['sessions'].pop()
        with self.assertRaises(ValueError):self.check()
        self.records=copy.deepcopy(self.original)
        self.records['anchor_preflight.json']['sessions'][0]['reply']['validation']['currents'][0]['torques'][0]+=.001
        with self.assertRaises(ValueError):self.check()

    def test_changed_declared_anchor_comparison_or_source_is_refused(self):
        self.records['anchor_preflight.json']['sessions'][0]['comparison']['F_relative_error']=0.
        with self.assertRaises(ValueError):self.check()
        self.records=copy.deepcopy(self.original)
        self.records['anchor_preflight.json']['source_sha256'].pop(next(iter(self.records['anchor_preflight.json']['source_sha256'])))
        with self.assertRaises(ValueError):self.check()

    def test_missing_replaced_or_wrong_bits_start_is_refused(self):
        row=self.records['start_preflight.json']['sessions'][0]
        row['checks'].pop()
        with self.assertRaises(ValueError):self.check()
        self.records=copy.deepcopy(self.original)
        self.records['start_preflight.json']['sessions'][0]['checks'][0]['x_bits'][0]+=1
        with self.assertRaises(ValueError):self.check()

    def test_start_geometry_or_mesh_failures_are_not_replacements(self):
        for mode in ('guard','quality','cost'):
            self.records=copy.deepcopy(self.original)
            r=self.records['start_preflight.json']['sessions'][0]['checks'][0]['reply']
            if mode=='guard':r['geometry']['guards'][0]['h']+=.1
            elif mode=='quality':r['quality']['min_angle_deg']=4.9
            else:r['charges'][0]['forward_solves']=1
            with self.subTest(mode=mode),self.assertRaises(ValueError):self.check()

    def test_missing_calibration_process_or_cold_repeat_is_refused(self):
        c=self.records['calibration_primary.json']['calibration'];c['per_process'].pop()
        with self.assertRaises(ValueError):self.check()
        self.records=copy.deepcopy(self.original)
        self.records['calibration_primary.json']['calibration']['per_process'][0]['validation_seconds'].pop()
        with self.assertRaises(ValueError):self.check()

    def test_primary_unit_and_l0_reuse_cannot_be_changed(self):
        for file in ('calibration_primary.json','calibration_secondary.json'):
            self.records=copy.deepcopy(self.original);self.records[file]['calibration']['unit_seconds']+=1.
            with self.subTest(file=file),self.assertRaises(ValueError):self.check()

    def test_calibration_weights_and_training_values_are_recomputed(self):
        for field in ('weights','theta0_train'):
            self.records=copy.deepcopy(self.original);c=self.records['calibration_primary.json']['calibration']
            if field=='weights':c[field]['train_forward_eval']+=.01
            else:c[field]['F']+=.01
            with self.subTest(field=field),self.assertRaises(ValueError):self.check()

    def test_unknown_host_evidence_and_failed_gate_are_refused(self):
        for field,value in (('host_contaminated',None),('monitor_error','lost samples'),('gate',False),('pre_run_load',17.)):
            self.records=copy.deepcopy(self.original)
            self.records['calibration_primary.json']['calibration']['host'][field]=value
            with self.subTest(field=field),self.assertRaises(ValueError):self.check()

    def test_original_setup_binary_runtime_or_mesh_disagreement_is_refused(self):
        for field in ('binary','runtime','mesh'):
            self.records=copy.deepcopy(self.original);b=self.records['bindings.json']
            if field=='binary':b['provenance']['evaluator']['sha256']='a'*64
            elif field=='runtime':b['provenance']['runtime']={}
            else:b['meshes']['validation']['sha256']='a'*64
            with self.subTest(field=field),self.assertRaises(ValueError):self.check()


if __name__=='__main__':unittest.main(verbosity=2)
