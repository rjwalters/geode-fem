"""Fresh conforming reporting, using retained native success/unavailable files."""
import copy
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import test_native_results as native_fixture
from test_design_evidence import digest
from selection import Validation, select
from native_results import requests, _conforming_gate
from check_plan import make_plan


class ConformingResultsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        native_fixture.NativeResultsTests.setUpClass()
        cls.fixture=native_fixture.NativeResultsTests()
        f=cls.fixture
        path=f.root/'native/a-conforming/result.json'
        raw=json.loads(path.read_text())
        cls.key=f.keys['a','theta0']+'/conforming'
        cls.artifacts=copy.deepcopy(f.artifacts)
        cls.artifacts[cls.key]={'path':path,'sha256':digest(path),
                               'geometry':f.geometry[f.keys['a','theta0']],
                               'meshes':{r['sha256']:path.parent/'meshes'/Path(r['file']).name
                                         for r in raw['fresh_mesh_records']}}

    @classmethod
    def tearDownClass(cls):
        native_fixture.NativeResultsTests.tearDownClass()

    def setUp(self):
        self.scratch=tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)

    def report(self,artifacts=None,**kwargs):
        return self.fixture.run_report(self.artifacts if artifacts is None else artifacts,
                                       include_conforming=True,**kwargs)

    def altered(self,mutate):
        artifact=copy.deepcopy(self.artifacts[self.key]);original=Path(artifact['path'])
        path=Path(self.scratch.name)/'result.json'
        shutil.copyfile(original.parent/'request.json',path.parent/'request.json')
        raw=json.loads(original.read_text());mutate(raw)
        path.write_text(json.dumps(raw,allow_nan=False));artifact.update(path=path,sha256=digest(path))
        return {self.key:artifact}

    def test_full_plan_schedules_matching_references_without_granting_eligibility(self):
        f=self.fixture;plan=make_plan(f.selection,include_conforming=True)
        self.assertEqual(len(plan['jobs']),28)
        self.assertEqual(len([j for j in plan['jobs'] if j['spec']['name']=='conforming']),4)
        jobs={j['job_id']:j for j in plan['jobs']}
        for j in jobs.values():
            self.assertEqual(j['spec'],jobs[j['theta0_job_id']]['spec'])
            self.assertEqual(j['status'],'not_run')
            if j['spec']['name']=='conforming':
                self.assertIsNone(j['spec']['harmonic_max'])
                self.assertEqual(j['spec']['mesh_origin'],'fresh_candidate')
        prepared=requests(f.selection,f.designs,f.definitions,f.meshes,include_conforming=True)
        native=json.loads((Path(self.artifacts[self.key]['path']).parent/'request.json').read_text())
        self.assertEqual(prepared['requests'][self.key],native)
        with self.assertRaises(ValueError):make_plan(f.selection,include_conforming=1)

    def test_real_fresh_conforming_output_joins_all_waveform_comparisons(self):
        out=self.report();row=out['observations'][self.key]
        self.assertEqual(sum(r['status']=='ok' for r in out['reductions'].values()),10)
        self.assertEqual(sum(r['status']=='not_run' for r in out['reductions'].values()),18)
        self.assertEqual(sum(len(c['torques']) for r in out['observations'].values() for c in r['currents']),640)
        self.assertEqual(row['forced_fresh_attempts'][-1]['outcome'],'passed')
        delta=out['comparisons'][self.key]['versus_primary']
        self.assertEqual(delta['status'],'ok')
        self.assertTrue(all(len(c['waveform_change']['torques'])==20 for c in delta['currents']))
        self.assertGreater(delta['F_change'],0.)
        self.assertFalse(out['can_select']);self.assertFalse(out['can_launch'])
        self.assertEqual(out['target_credit_policy'],'preserve_original_common_validation')

    def test_missing_fresh_history_or_harmonic_mesh_substitution_is_refused(self):
        mutations=[lambda r:r.pop('forced_fresh_attempts'),
                   lambda r:r['forced_fresh_attempts'].clear(),
                   lambda r:r['fresh_mesh_records'].clear(),
                   lambda r:r['validation']['mesh'].update(kind='moved'),
                   lambda r:r['validation'].update(msh_sha256=self.fixture.meshes[2]['sha256']),
                   lambda r:r['validation'].update(remesh_mesh_s=1.),
                   lambda r:r['config'].update(mode='h93'),
                   lambda r:r['config'].update(torque='h93/coupling')]
        for i,mutation in enumerate(mutations):
            with self.subTest(i=i),self.assertRaises(ValueError):self.report(self.altered(mutation))

    def test_mesh_order_quality_files_and_terminal_pass_are_verified(self):
        mutations=[lambda r:r['forced_fresh_attempts'][0].update(label='size_factor_0.8'),
                   lambda r:r['forced_fresh_attempts'][0]['quality'].update(min_angle_deg=4.),
                   lambda r:r['forced_fresh_attempts'][0].update(outcome='policy_failed'),
                   lambda r:r['forced_fresh_attempts'][0].update(msh_sha256='a'*64),
                   lambda r:r['fresh_mesh_records'][0].update(error='mesher failed'),
                   lambda r:r['forced_fresh_attempts'].append(copy.deepcopy(r['forced_fresh_attempts'][0]))]
        for i,mutation in enumerate(mutations):
            with self.subTest(i=i),self.assertRaises(ValueError):self.report(self.altered(mutation))
        artifact=copy.deepcopy(self.artifacts[self.key]);h=next(iter(artifact['meshes']))
        artifact['meshes'][h]=self.fixture.meshes[2]['path']
        with self.assertRaises(ValueError):self.report({self.key:artifact})

    def test_rotor_offsets_are_allowed_but_stator_and_unsupported_angles_are_not(self):
        f=self.fixture;adapted=copy.deepcopy(f.designs[f.keys['b','theta0']])
        spec=next(s for s in make_plan(f.selection,include_conforming=True)['specs'] if s['name']=='conforming')
        adapted['parameters']['CPR1']=.0001
        _conforming_gate(adapted,spec)
        adapted['parameters']['CPS1']=-0.0
        _conforming_gate(adapted,spec)
        adapted['parameters']['CPS1']=1e-100
        with self.assertRaises(ValueError):_conforming_gate(adapted,spec)
        adapted['parameters']['CPS1']=0.0;spec['angles_deg']=[.01]
        with self.assertRaises(ValueError):_conforming_gate(adapted,spec)

    def test_real_nonzero_stator_unavailable_keeps_frozen_design_and_cost(self):
        f=self.fixture;retained=json.loads((f.root/'native/b-retained-ready.json').read_text())
        ready=retained['ready'];x=copy.deepcopy(ready['space']['x0']);x[15]=.0001
        provenance={k:retained['checks'][0]['adapted']['settings'][k]
                    for k in ('source_commit','binary_sha256','source_files_sha256')}
        adapted=f.definitions.adapt({'space':'b','evaluator_ready':ready,'anchor':ready['anchor']},
                                   {'x':x},provenance=provenance)
        design=adapted['design'];values=[]
        for space in 'ab':
            values.append(Validation(space,'a'*64,'A',space,0,1,0,f.designs[f.keys[space,'lhs1']]['design'],
                                     10.,120.,.001,2.,eligible_run=True,host_contaminated=False,completed=True,
                                     geometry_admissible=True,discrete_pass=True,mesh_pass=True))
        values.append(Validation('undefined','b'*64,'B','b',0,1,0,design,10.,120.,.001,2.,
                                 eligible_run=True,host_contaminated=False,completed=True,
                                 geometry_admissible=True,discrete_pass=True,mesh_pass=True))
        selection=select(values,f0=.01,mean0=1.4,theta0=f.designs[f.keys['a','theta0']]['design'],
                         theta0_references=[f.designs[f.keys['b','theta0']]['design']])
        designs=dict(f.designs);designs[design.key]=adapted
        path=f.root/'native/nonzero-stator/result.json';key=design.key+'/conforming'
        artifacts=dict(self.artifacts);artifacts[key]={'path':path,'sha256':digest(path)}
        out=self.report(artifacts,selection=selection,designs=designs)
        row=out['observations'][key]
        self.assertEqual(row['status'],'unavailable')
        self.assertIn('model_dependent',row['reason'])
        self.assertEqual(row['failure_evidence']['request']['design_key'],design.key)
        self.assertGreater(row['cost']['wall_seconds'],0.)
        self.assertFalse(row['cost']['counted_in_run_budget'])
        self.assertEqual(out['comparisons'][key]['target_check']['status'],'unavailable')
        self.assertEqual(out['cohort'],selection['jobs'])


if __name__=='__main__':unittest.main(verbosity=2)
