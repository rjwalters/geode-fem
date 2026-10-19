"""Replay real native outputs with a synthetic selection context; no new FEM.

Archived waveforms, requests and geometry remain unchanged. Meshes are regenerated
from pinned source and checked against the original byte digests. The two lhs1
points are put into the cohort with invented feasible selection values solely to
exercise reporting of their actual, infeasible final-check outcomes. This fixture
is neither a closed main study nor evidence of optimizer success.
"""
import copy
from mesh_test_support import mesh_bytes, hydrate
import json
from pathlib import Path
import shutil
import sys
import tarfile
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from test_design_evidence import definitions, digest
from selection import Validation, select
from native_results import requests, report


class NativeResultsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        cls.definitions = definitions()
        for name, target in [('native_refinement_63b3e3d8.tar.gz', 'native'),
                             ('native_matrix_63b3e3d8.tar.gz', 'matrix')]:
            pinned = {'native_refinement_63b3e3d8.tar.gz':'38b579ae8a76bf49617572afd4e0e145c8cdecc549b018a9a7837f6399a3970a',
                      'native_matrix_63b3e3d8.tar.gz':'f2d40092edce01a8227cdbb27ab92a534ecfc04174a4d21f2dc88c0d5bf6948a'}
            assert digest(HERE/'smoke'/name) == pinned[name]
            with tarfile.open(HERE/'smoke'/name) as archive:
                archive.extractall(cls.root/target, filter='data')
            hydrate(cls.root/target)
        l2 = cls.root/'L2.msh'
        l2.write_bytes(mesh_bytes('00a7eb6f88bd8fdf22e8ad0b406d4889a68c2d269fc3216ff81f255cddb71cf5'))
        assert digest(l2) == '00a7eb6f88bd8fdf22e8ad0b406d4889a68c2d269fc3216ff81f255cddb71cf5'
        l3 = cls.root/'matrix/l3-mesh/icem_initial_sector_L3.msh'
        cls.meshes = {2: {'path':l2, 'sha256':digest(l2)}, 3: {'path':l3, 'sha256':digest(l3)}}
        cls.designs, cls.keys, cls.geometry, cls.geometry_definitions = {}, {}, {}, {}
        values, refs = [], []
        for space in 'ab':
            retained = json.loads((cls.root/f'native/{space}-retained-ready.json').read_text())
            ready = retained['ready']
            declaration = {k:retained['checks'][0]['adapted']['settings'][k]
                           for k in ('source_commit','binary_sha256','source_files_sha256')}
            for start in ('theta0','lhs1'):
                source = next(c for c in retained['checks'] if c['start']==start)
                geo = source['reply']
                adapted = cls.definitions.adapt({'space':space,'evaluator_ready':ready,'anchor':ready['anchor']},
                                               geo, provenance=declaration)
                key = adapted['design'].key
                cls.designs[key] = adapted
                cls.keys[space,start] = key
                path = cls.root/f'{space}-{start}-geometry.json'
                path.write_text(json.dumps(geo, allow_nan=False))
                cls.geometry[key] = {'path':path,'sha256':digest(path),'space':space}
                if start=='theta0':
                    refs.append(adapted['design'])
                    cls.geometry_definitions[space] = {
                        'guard_specs': {g['name']:{k:g[k] for k in ('scale','strict','unit')}
                                        for g in geo['geometry']['guards']},
                        'discrete_names': [g['name'] for g in geo['geometry']['discrete']]}
                else:
                    # Deliberately synthetic prior selection; actual final means fail.
                    values.append(Validation(space,'a'*64,'A',space,0,1,0,adapted['design'],10.,120.,.001,2.,
                                             eligible_run=True,host_contaminated=False,completed=True,
                                             geometry_admissible=True,discrete_pass=True,mesh_pass=True))
        cls.selection = select(values, f0=.01, mean0=1.4, theta0=refs[0], theta0_references=[refs[1]])
        receipt = json.loads((cls.root/'native/build-receipt.json').read_text())
        cls.provenance = {'source_commit':'63b3e3d8bbc30504f845eebb5b78e8be6a2ee12f',
                          'binary_sha256':receipt['binary_sha256'], 'common_physics_sha256':cls.definitions.reference_sha256}
        cls.artifacts = {}
        cases = [('native','a-primary','a','theta0','primary'),('native','b-primary','b','theta0','primary'),
                 ('native','a-shifted','a','theta0','shifted')]
        cases += [('matrix','a-theta0-'+check,'a','theta0',check) for check in ('g40','tight','withheld','L3')]
        cases += [('matrix',s+'-lhs1-primary',s,'lhs1','primary') for s in 'ab']
        for folder,name,space,start,check in cases:
            path=cls.root/f'{folder}/{name}/result.json';raw=json.loads(path.read_text())
            mesh_paths={m['sha256']:m['path'] for m in cls.meshes.values()}
            for m in raw['fresh_mesh_records']:
                if m['sha256']:
                    mesh_paths[m['sha256']]=path.parent/'meshes'/Path(m['file']).name
            key=cls.keys[space,start]
            cls.artifacts[key+'/'+check]={'path':path,'sha256':digest(path),
                                        'geometry':cls.geometry[key], 'meshes':mesh_paths}

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def setUp(self):
        self.scratch=tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)

    def run_report(self, artifacts=None, **overrides):
        args=dict(selection=self.selection,designs=self.designs,definitions=self.definitions,
                  meshes=self.meshes,artifacts=self.artifacts if artifacts is None else artifacts,
                  provenance=self.provenance,geometry_definitions=self.geometry_definitions)
        args.update(overrides)
        return report(**args)

    def altered(self, mutate, *, space='a', start='theta0', check='primary'):
        key=self.keys[space,start]+'/'+check
        artifact=copy.deepcopy(self.artifacts[key]);original=Path(artifact['path'])
        path=Path(self.scratch.name)/'result.json'
        shutil.copyfile(original.parent/'request.json',path.parent/'request.json')
        raw=json.loads(original.read_text());mutate(raw)
        path.write_text(json.dumps(raw,allow_nan=False))
        artifact.update(path=path,sha256=digest(path))
        return {key:artifact}

    def test_exact_requests_reproduce_retained_native_requests(self):
        prepared=requests(self.selection,self.designs,self.definitions,self.meshes)
        self.assertFalse(prepared['can_launch']);self.assertEqual(len(prepared['requests']),24)
        for key,artifact in self.artifacts.items():
            retained=json.loads((Path(artifact['path']).parent/'request.json').read_text())
            self.assertEqual(prepared['requests'][key],retained)

    def test_real_matrix_reports_missing_checks_and_infeasible_candidates(self):
        out=self.run_report()
        self.assertEqual(sum(v['status']=='ok' for v in out['reductions'].values()),9)
        self.assertEqual(sum(v['status']=='not_run' for v in out['reductions'].values()),15)
        self.assertEqual(sum(len(c['torques']) for r in out['observations'].values() for c in r['currents']),580)
        self.assertEqual(out['cohort'],self.selection['jobs'])
        self.assertFalse(out['can_select']);self.assertFalse(out['can_launch'])
        self.assertTrue(all(not rows for rows in out['feasible_orderings'].values()))
        for s in 'ab':
            key=self.keys[s,'lhs1']+'/primary'
            self.assertFalse(out['comparisons'][key]['target_check']['feasible'])
            self.assertFalse(any(out['comparisons'][key]['target_check']['targets'].values()))
        key=self.keys['a','theta0']
        self.assertIsNone(out['reductions'][key+'/withheld']['F'])
        shifted=out['comparisons'][key+'/shifted']['versus_primary']['currents']
        self.assertTrue(all(c['waveform_change']['status']=='unavailable' for c in shifted))
        g40=out['comparisons'][key+'/g40']['versus_primary']['currents']
        self.assertTrue(all(len(c['waveform_change']['torques'])==20 for c in g40))

    def test_changed_configuration_coordinates_request_or_scope_is_refused(self):
        mutations=[lambda r:r['config'].update(newton_rel_tol=1e-12),
                   lambda r:r['validation']['point'].__setitem__(0,.009),
                   lambda r:r['point_bits'].__setitem__(-2,False),
                   lambda r:r['request'].update(check='tight'),
                   lambda r:r.update(cost_scope='in_run'),
                   lambda r:r.update(target_credit_policy='recompute')]
        for i,mutation in enumerate(mutations):
            with self.subTest(i=i),self.assertRaises(ValueError):self.run_report(self.altered(mutation))

    def test_incomplete_waveforms_and_changed_aggregates_are_refused(self):
        mutations=[lambda r:r['validation']['currents'][0]['torques'].pop(),
                   lambda r:r['validation']['currents'].pop(),
                   lambda r:r['validation']['currents'][0].update(mean=10.),
                   lambda r:r['validation'].update(F=.001),
                   lambda r:r['validation'].update(full_current_mean=5.)]
        for i,mutation in enumerate(mutations):
            with self.subTest(i=i),self.assertRaises(ValueError):self.run_report(self.altered(mutation))

    def test_hashes_and_geometry_map_are_checked_against_files(self):
        for change in ('raw','geometry','mesh','geometry_space'):
            artifacts=copy.deepcopy(self.artifacts)
            key=self.keys['a','theta0']+'/primary';a=artifacts[key]
            if change=='raw':a['sha256']='a'*64
            elif change=='geometry':a['geometry']['sha256']='a'*64
            elif change=='mesh':a['meshes'][self.meshes[2]['sha256']]=self.meshes[3]['path']
            else:a['geometry']['space']='b'
            with self.subTest(change=change),self.assertRaises(ValueError):self.run_report(artifacts)

    def test_geometry_coordinates_normalization_and_completeness_are_checked(self):
        for mutation in [lambda g:g['x'].__setitem__(0,.009),
                         lambda g:g['geometry']['guards'][0].update(h=10.),
                         lambda g:g['geometry']['guards'].pop(),
                         lambda g:g['geometry']['discrete'].pop()]:
            key=self.keys['a','theta0']+'/primary'
            artifact=copy.deepcopy(self.artifacts[key]);binding=artifact['geometry']
            geo=json.loads(Path(binding['path']).read_text());mutation(geo)
            path=Path(self.scratch.name)/'geometry.json';path.write_text(json.dumps(geo))
            binding.update(path=path,sha256=digest(path))
            with self.assertRaises(ValueError):self.run_report({key:artifact})

    def test_fresh_mesh_history_and_resource_totals_are_checked(self):
        for mutation in [lambda r:r['pre_solve_mesh_attempts'].clear(),
                         lambda r:r['fresh_mesh_records'][0].update(sha256='a'*64),
                         lambda r:r['validation']['mesh']['attempts'][0].update(outcome='policy_failed')]:
            with self.assertRaises(ValueError):
                self.run_report(self.altered(mutation,space='b',start='lhs1'))
        for mutation in [lambda r:r['resources'].update(observed_cpu_seconds=0.),
                         lambda r:r['resources'].update(cpu_complete=False),
                         lambda r:r['resources']['after'].update(pid=1),
                         lambda r:r.update(wall_seconds=0.)]:
            with self.assertRaises(ValueError):self.run_report(self.altered(mutation))

    def test_failed_and_unavailable_outcomes_are_retained_without_replacement(self):
        for status in ('failed','refused','unavailable'):
            artifacts=self.altered(lambda r:r.update(ok=False,status=status,error='injected failure'))
            out=self.run_report(artifacts)
            key=next(iter(artifacts));row=out['observations'][key]
            self.assertEqual(row['native_status'],status)
            self.assertEqual(out['cohort'],self.selection['jobs'])
            self.assertEqual(sum(r['status']=='not_run' for r in out['reductions'].values()),23)
            self.assertEqual(row['failure_evidence']['error'],'injected failure')
            self.assertFalse(row['cost']['counted_in_run_budget'])
            self.assertEqual(out['comparisons'][key]['target_check']['status'],'unavailable')

    def test_unfrozen_designs_meshes_and_out_of_plan_artifacts_are_refused(self):
        bad=copy.deepcopy(self.designs);bad[self.keys['b','theta0']]['native_point_bits'][-2]=False
        with self.assertRaises(ValueError):self.run_report(designs=bad)
        bad=copy.deepcopy(self.meshes);bad[2]=bad[3]
        with self.assertRaises(ValueError):self.run_report(meshes=bad)
        bad=copy.deepcopy(self.artifacts);bad['unknown']=next(iter(bad.values()))
        with self.assertRaises(ValueError):self.run_report(bad)
        with self.assertRaises(ValueError):self.run_report(provenance={})


if __name__=='__main__':
    unittest.main(verbosity=2)
