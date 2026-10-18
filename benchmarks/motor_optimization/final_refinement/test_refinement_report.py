"""Real retained native outputs in a synthetic finalized-execution layout."""
import copy
import json
from pathlib import Path
import shutil
import sys
import tempfile
import tarfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import refinement_report as reporting
import refinement_bundle
import native_results
import test_native_results
import test_design_evidence


class RefinementReportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        fixture = test_native_results.NativeResultsTests
        fixture.setUpClass(); cls.fixture = fixture
        cls.temp = tempfile.TemporaryDirectory(); cls.base = Path(cls.temp.name).resolve()
        inputs = cls.base / 'bundle'; (inputs / 'inputs').mkdir(parents=True)
        for source, name in ((test_design_evidence.REFERENCE, 'reference_data.json'),
                             (test_design_evidence.STARTS, 'starts.json')):
            shutil.copyfile(source, inputs / 'inputs' / name)
        for level, mesh in fixture.meshes.items():
            shutil.copyfile(mesh['path'], inputs / f'inputs/L{level}.msh')
        prepared = native_results.requests(fixture.selection, fixture.designs, fixture.definitions,
                                          fixture.meshes, include_conforming=True)
        designs = copy.deepcopy(fixture.designs)
        for value in designs.values():
            for key in ('design', 'theta0'): value[key] = value[key].canonical
        result = dict(selection=fixture.selection, plan=prepared['plan'], designs=designs,
                      geometry_definitions=fixture.geometry_definitions, attempts=[{'scope': 'synthetic selection fixture'}])
        (inputs / 'selection.json').write_text(json.dumps(result))
        jobs, geometries = refinement_bundle._schedule(result, fixture.definitions, fixture.meshes)
        cls.bundle = dict(reference_sha256=fixture.definitions.reference_sha256,
            starts_sha256=fixture.definitions.starts_sha256, L2_sha256=fixture.meshes[2]['sha256'],
            mesh_evidence={'mesh_sha256': fixture.meshes[3]['sha256']},
            jobs=[{k: v for k, v in j.items() if k != 'request'} for j in jobs],
            geometry=[{k: v for k, v in g.items() if k != 'request'} for g in geometries])
        root = cls.base / 'execution'; root.mkdir(); rows = []
        for geo in geometries:
            key = geo['design_key']; folder = root / 'geometry' / key; folder.mkdir(parents=True)
            shutil.copyfile(fixture.geometry[key]['path'], folder / 'geometry.json')
            geometry=json.loads((folder/'geometry.json').read_text())
            (folder / 'process.json').write_text(json.dumps({'returncode': 0, 'wall_seconds': .01,
                'pid':geometry['resources']['before']['pid'],'fixture': True}))
            rows.append(dict(kind='geometry', key=key, output_directory='geometry/'+key, returncode=0, status='retained'))
        for job in jobs:
            key = job['job_id']; folder = root / job['output_directory']; folder.mkdir(parents=True)
            artifact = fixture.artifacts.get(key)
            if artifact:
                shutil.copytree(Path(artifact['path']).parent, folder / 'native')
            outcome = dict(kind='native',key=key,output_directory=job['output_directory'],
                           returncode=0 if artifact else -9,status='retained' if artifact else 'unavailable')
            (folder / 'outcome.json').write_text(json.dumps(outcome))
            (folder / 'process.json').write_text(json.dumps({'returncode':outcome['returncode'],
                'wall_seconds':1.,'fixture':True,'pid':json.loads((folder/'native/result.json').read_text())['resources']['before']['pid'] if artifact else 12345}))
            rows.append(outcome)
        cls.metadata = {'rows': rows}
        witness = cls.base / 'witness'
        archive = HERE / 'smoke/runner_native_1885192d.tar.gz'
        assert reporting.producer_source.digest(archive) == '664c9a2facce6e2b41c0e37970a967805096baf86f0cd82e2fa8d4dfceb5afbd'
        with tarfile.open(archive) as tar:
            tar.extractall(witness, filter='data')
        shutil.copyfile(witness/'environment-current.json', witness/'execution/environment-original.json')
        for name in ('environment-before.json','environment-after.json'):
            shutil.copyfile(witness/name,witness/'execution'/name)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup(); cls.fixture.tearDownClass()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve() / 'execution'
        shutil.copytree(self.base / 'execution', self.root)
        self.metadata = copy.deepcopy(type(self).metadata)

    def report(self):
        return reporting._scientific(self.root,self.base/'bundle',self.bundle,self.metadata,
                                     provenance=self.fixture.provenance)

    def native(self, check='primary'):
        key=self.fixture.keys['a','theta0']+'/'+check
        row=next(r for r in self.metadata['rows'] if r['kind']=='native' and r['key']==key)
        return key,row,self.root/row['output_directory']

    def test_full_report_retains_real_waveforms_and_explicit_missing_outcomes(self):
        before=copy.deepcopy(self.fixture.selection)
        report=self.report()
        self.assertEqual(sum(v['status']=='ok' for v in report['reductions'].values()),9)
        self.assertEqual(sum(v['status']=='unavailable' for v in report['reductions'].values()),19)
        self.assertEqual(sum(len(c['torques']) for r in report['observations'].values()
                             if r['status']=='ok' for c in r['currents']),580)
        self.assertEqual(len(report['execution_audit']),28);self.assertEqual(len(report['geometry_costs']),4)
        self.assertEqual(self.fixture.selection,before)
        self.assertEqual(report['selection_provenance'],[{'scope':'synthetic selection fixture'}])
        self.assertFalse(report['can_select']);self.assertFalse(report['can_launch'])
        self.assertTrue(all(not rows for rows in report['feasible_orderings'].values()))

    def test_geometry_failure_keeps_native_result_but_makes_comparison_unavailable(self):
        key,row,folder=self.native()
        geo=next(r for r in self.metadata['rows'] if r['kind']=='geometry' and r['key']==self.fixture.keys['a','theta0'])
        geo['status']='unavailable'
        report=self.report();self.assertEqual(report['reductions'][key]['status'],'unavailable')
        audit=next(r for r in report['execution_audit'] if r['job_id']==key)
        self.assertIsNotNone(audit['raw_result_sha256'])
        self.assertEqual(report['comparisons'][key]['target_check']['status'],'unavailable')

    def test_abnormal_exit_does_not_promote_a_retained_complete_result(self):
        key,row,folder=self.native();row['returncode']=-9
        report=self.report();self.assertEqual(report['reductions'][key]['status'],'unavailable')

    def test_native_exit_and_success_flag_must_agree(self):
        key,row,folder=self.native();row['returncode']=2
        with self.assertRaisesRegex(ValueError,'contradicts its exit'):self.report()

    def test_native_resource_identity_must_match_owned_process(self):
        key,row,folder=self.native();path=folder/'process.json'
        receipt=json.loads(path.read_text());receipt['pid']+=1;path.write_text(json.dumps(receipt))
        with self.assertRaisesRegex(ValueError,'owned child receipt'):self.report()

    def test_wrong_native_request_is_refused(self):
        key,row,folder=self.native();path=folder/'native/result.json'
        raw=json.loads(path.read_text());raw['request']['check']='shifted';path.write_text(json.dumps(raw))
        with self.assertRaisesRegex(ValueError,'changes request'):self.report()

    def test_changed_waveform_statistics_are_refused(self):
        key,row,folder=self.native();path=folder/'native/result.json'
        raw=json.loads(path.read_text());raw['validation']['currents'][0]['torques'][0]+=1
        path.write_text(json.dumps(raw))
        with self.assertRaisesRegex(ValueError,'aggregates disagree'):self.report()

    def test_missing_planned_outcome_cannot_disappear_from_summary(self):
        self.metadata['rows'].pop()
        with self.assertRaisesRegex(ValueError,'scheduled outcome'):self.report()

    def test_public_loader_refuses_unfinalized_evidence_before_scientific_read(self):
        final=self.root.parent/'final.json';final.write_text(json.dumps(dict(schema=1,
            kind='final_refinement_execution_finalized',can_report=False,can_launch=False,
            execution={'sha256':'a'*64})))
        with (mock.patch.object(reporting.refinement_run,'_manifest',return_value=self.metadata),
              mock.patch.object(reporting,'_scientific') as scientific):
            with self.assertRaisesRegex(ValueError,'does not authorize'):
                reporting.load(self.root,execution_sha256='a'*64,finalization_path=final,
                    finalization_sha256=reporting.producer_source.digest(final),bundle_root=self.base/'bundle',
                    bundle_sha256='b'*64,build_root=self.root,build_sha256='c'*64,git_repo=HERE.parents[2],executor_commit='d'*40)
            scientific.assert_not_called()

    def test_independent_finalization_digest_is_required(self):
        path=self.root.parent/'final.json';path.write_text('{}')
        with self.assertRaisesRegex(ValueError,'retained receipt'):reporting._read(path,'a'*64)

    def witnessed_environment(self):
        root=self.base/'witness'
        original=json.loads((root/'environment-current.json').read_text())
        bound=original['bindings'];commit=bound['build']['source']['commit']
        metadata={'arguments':dict(bundle_sha256=bound['bundle']['sha256'],build_sha256=bound['build']['sha256'],
            executor_commit=commit,bundle_root=bound['bundle']['directory'],build_root=bound['build']['directory'],
            repo=bound['build']['source']['repo']),
            'environment_sha256':reporting.producer_source.digest(root/'environment-current.json')}
        final={'current_environment':json.loads((root/'environment-after.json').read_text()),
               'processes':json.loads((root/'process-verification.json').read_text())}
        bundle={'capture_manifest_sha256':bound['bundle']['capture_manifest_sha256'],
            'reviewed_producer_commit':bound['bundle']['producer_commit'],'reference_sha256':bound['reference_sha256'],
            'mesh_evidence':{'construction_source_sha256':bound['construction_source_sha256']}}
        return root,metadata,final,bundle,bound['build'],bound['bundle']['sha256'],bound['build']['sha256'],commit

    def test_real_environment_observations_bind_builds_and_reject_drift(self):
        root,metadata,final,bundle,build,bsha,esha,commit=self.witnessed_environment()
        reporting._environment(root/'execution',metadata,final,bundle,build,bsha,esha,commit)
        changed=copy.deepcopy(final);changed['current_environment']['runtime']['controls']['GEODE_NUM_THREADS']='2'
        with self.assertRaisesRegex(ValueError,'observations disagree'):
            reporting._environment(root/'execution',metadata,changed,bundle,build,bsha,esha,commit)
        changed=copy.deepcopy(build);changed['executables']['icem_final_refinement']['binary_sha256']='f'*64
        with self.assertRaisesRegex(ValueError,'executable receipt changed'):
            reporting._environment(root/'execution',metadata,final,bundle,changed,bsha,esha,commit)
        with self.assertRaisesRegex(ValueError,'input/build identities'):
            reporting._environment(root/'execution',metadata,final,bundle,build,'f'*64,esha,commit)

    def test_historical_real_process_verification_rejects_forged_observations(self):
        root=self.base/'witness'
        final={'processes':json.loads((root/'process-verification.json').read_text())}
        metadata={'rows':json.loads((root/'rows.json').read_text())}
        bundle=json.loads((root/'smoke-plan.json').read_text())
        # Added environment records are outside per-request inventories.
        commit='1c6784e7e1fc8fe0564b1d793a6c4061588028f5'
        reporting._historical(root/'execution',final,metadata,bundle,HERE.parents[2],commit)
        changed=copy.deepcopy(final);changed['processes']['report']['processes_stopped']=False
        with self.assertRaisesRegex(ValueError,'do not replay'):
            reporting._historical(root/'execution',changed,metadata,bundle,HERE.parents[2],commit)


if __name__ == '__main__': unittest.main(verbosity=2)
