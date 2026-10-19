"""Synthetic complete-waveform comparisons; no FEM evidence."""
import copy
import math
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parent))
from check_plan import make_plan
from comparison import summarize, difference
from test_check_plan import manifest, multiple_settings_manifest


def records(selection):
    plan=make_plan(selection)
    anchor=next(c['design_key'] for c in plan['cohort'] if 'theta0' in c['roles'])
    rows=[]
    for job in plan['jobs']:
        initial=job['design_key'] in plan['reference_design_keys']
        mean=1.0 if initial else 1.01
        amp=.001 if initial else .0002
        currents=[]
        for fraction in job['spec']['current_fractions']:
            torques=[mean+amp*(-1 if i%2 else 1) for i in range(len(job['spec']['angles_deg']))]
            m=math.fsum(torques)/len(torques)
            std=math.sqrt(math.fsum((v-m)**2 for v in torques)/len(torques))
            currents.append({'fraction':fraction,'torques':torques,'mean':m,'std':std})
        rows.append({'job_id':job['job_id'],'design_key':job['design_key'],'raw_sha256':'a'*64,
                     'status':'ok','actual_spec':copy.deepcopy(job['spec']),
                     'provenance':{'source_commit':'b'*40,'binary_sha256':'c'*64,'common_physics_sha256':'d'*64},
                     'currents':currents,'sum_variances':sum(c['std']**2 for c in currents),
                     'geometry':{'buildable':True,'guards':[{'h':.1}],'discrete':[{'pass':True}]},
                     'mesh_pass':True,'cost':{'counted_in_run_budget':False,'wall_seconds':2.,'cpu':None,'rss':None}})
    return rows,anchor


class ComparisonTests(unittest.TestCase):
    def test_reduces_every_waveform_and_preserves_cohort_credit_and_costs(self):
        selection=manifest();rows,anchor=records(selection);before=copy.deepcopy((selection,rows))
        report=summarize(selection,rows)
        self.assertEqual((selection,rows),before)
        self.assertEqual(len(report['reductions']),12)
        self.assertEqual(report['cohort'],selection['jobs'])
        self.assertEqual(report['target_credit_policy'],'preserve_original_common_validation')
        self.assertEqual(report['replacement_policy'],'never_replace_selected_design')
        candidate=next(r for r in rows if r['design_key']!=anchor and r['actual_spec']['name']=='primary')
        result=report['reductions'][candidate['job_id']]
        self.assertAlmostEqual(result['F'],3*.0002**2,places=16)
        comparison=report['comparisons'][candidate['job_id']]
        self.assertTrue(comparison['target_check']['targets']['T50'])
        self.assertFalse(comparison['target_check']['targets']['T90'])
        self.assertAlmostEqual(comparison['matching_theta0']['currents'][0]['mean_change'],.01)
        self.assertLess(comparison['matching_theta0']['F_change'],0)
        self.assertEqual(report['observations'][candidate['job_id']]['cost'],candidate['cost'])

    def test_cp_candidate_uses_cp_reference_without_changing_original_credit(self):
        selection=multiple_settings_manifest();rows,anchor=records(selection)
        before=copy.deepcopy(selection)
        cp_reference=next(c['design_key'] for c in selection['jobs'] if 'theta0_reference' in c['roles'])
        cp_candidate=next(c['design_key'] for c in selection['jobs'] if 'A/b/representative' in c['roles'])
        scalar_candidate=next(c['design_key'] for c in selection['jobs'] if 'A/a/representative' in c['roles'])
        for row in rows:
            if row['design_key']==cp_reference:
                for current in row['currents']:
                    current['torques']=[v+.2 for v in current['torques']]
                    current['mean']=math.fsum(current['torques'])/len(current['torques'])
        report=summarize(selection,rows)
        cp=report['comparisons'][cp_candidate+'/primary']
        scalar=report['comparisons'][scalar_candidate+'/primary']
        self.assertFalse(cp['target_check']['feasible'])
        self.assertTrue(scalar['target_check']['feasible'])
        self.assertAlmostEqual(cp['matching_theta0']['currents'][0]['mean_change'],-.19)
        self.assertEqual(selection,before)
        self.assertEqual(len(report['cohort']),4)
        self.assertTrue(all(r['design_key'] not in (anchor,cp_reference)
            for ranking in report['feasible_orderings'].values() for r in ranking))
        rows=[r for r in rows if r['design_key']!=cp_reference]
        missing=summarize(selection,rows)
        self.assertEqual(missing['comparisons'][cp_candidate+'/primary']['target_check']['status'],'unavailable')
        self.assertTrue(missing['comparisons'][scalar_candidate+'/primary']['target_check']['feasible'])

    def test_direct_waveform_comparison_refuses_different_base_settings(self):
        selection=multiple_settings_manifest();rows,_=records(selection)
        report=summarize(selection,rows)
        candidates=[r for r in rows if r['actual_spec']['name']=='primary']
        left=candidates[0]
        right=next(r for r in candidates if r['actual_spec']['base_settings_sha256']!=left['actual_spec']['base_settings_sha256'])
        with self.assertRaisesRegex(ValueError,'base design settings'):
            difference(left,right,report['reductions'])

    def test_missing_and_failed_checks_remain_unavailable_without_replacement(self):
        selection=manifest();rows,anchor=records(selection)
        failed=next(r for r in rows if r['design_key']!=anchor and r['actual_spec']['name']=='L3')
        failed.update(status='failed',reason='native solver failure')
        removed=next(r for r in rows if r['design_key']==anchor and r['actual_spec']['name']=='tight')
        rows.remove(removed)
        report=summarize(selection,rows)
        self.assertEqual(report['reductions'][failed['job_id']]['status'],'failed')
        self.assertEqual(report['reductions'][removed['job_id']]['status'],'not_run')
        self.assertEqual(report['comparisons'][failed['job_id']]['target_check']['status'],'unavailable')
        candidate=next(r for r in rows if r['design_key']!=anchor and r['actual_spec']['name']=='tight')
        self.assertEqual(report['comparisons'][candidate['job_id']]['target_check']['status'],'unavailable')
        self.assertEqual(report['feasible_orderings']['L3'],[])
        self.assertEqual(report['cohort'],selection['jobs'])

    def test_duplicate_replacement_and_foreign_job_are_refused(self):
        selection=manifest();rows,_=records(selection)
        with self.assertRaisesRegex(ValueError,'duplicate'):summarize(selection,rows+[copy.deepcopy(rows[0])])
        rows[0]['job_id']='foreign'
        with self.assertRaisesRegex(ValueError,'unknown'):summarize(selection,rows)

    def test_wrong_settings_current_grid_and_aggregates_are_refused(self):
        selection=manifest();rows,_=records(selection)
        for change in ('tolerance','initialization','current','short','nonfinite','mean','std','F'):
            changed=copy.deepcopy(rows);r=changed[0]
            if change=='tolerance':r['actual_spec']['newton_rel_tol']=1e-4
            elif change=='initialization':r['actual_spec']['current_sweep_initialization']='warm'
            elif change=='current':r['currents'][0]['fraction']=.25
            elif change=='short':r['currents'][0]['torques'].pop()
            elif change=='nonfinite':r['currents'][0]['torques'][0]=math.nan
            elif change in ('mean','std'):r['currents'][0][change]+=1e-4
            else:r['sum_variances']+=1e-4
            with self.subTest(change=change),self.assertRaises(ValueError):summarize(selection,changed)

    def test_comparisons_require_same_source_binary_and_physics(self):
        selection=manifest();rows,anchor=records(selection)
        for field in ('source_commit','binary_sha256','common_physics_sha256'):
            changed=copy.deepcopy(rows)
            candidate=next(r for r in changed if r['design_key']!=anchor)
            candidate['provenance'][field]='e'*(40 if field=='source_commit' else 64)
            with self.subTest(field=field),self.assertRaisesRegex(ValueError,'mixes source'):
                summarize(selection,changed)

    def test_withheld_currents_get_no_training_objective_or_target(self):
        selection=manifest();rows,_=records(selection);report=summarize(selection,rows)
        for r in rows:
            if r['actual_spec']['name']=='withheld':
                self.assertIsNone(report['reductions'][r['job_id']]['F'])
                self.assertIsNone(report['reductions'][r['job_id']]['mean_full'])
                self.assertEqual(report['comparisons'][r['job_id']]['target_check']['status'],'unavailable')
                self.assertEqual(report['comparisons'][r['job_id']]['matching_theta0']['status'],'ok')
        self.assertEqual(report['feasible_orderings']['withheld'],[])

    def test_waveform_changes_never_subtract_different_angles(self):
        selection=manifest();rows,anchor=records(selection);report=summarize(selection,rows)
        for r in rows:
            if r['design_key']==anchor:continue
            compare=report['comparisons'][r['job_id']]['versus_primary']
            if r['actual_spec']['name']=='shifted':
                self.assertEqual(compare['status'],'ok')
                for c in compare['currents']:
                    self.assertEqual(c['waveform_change']['status'],'unavailable')
                    self.assertEqual(c['waveform_change']['torques'],[])
            if r['actual_spec']['name']=='g40':
                self.assertEqual(compare['currents'][0]['waveform_change']['angles_deg'],list(range(20)))

    def test_credit_tolerance_mean_and_mesh_survival_are_independent(self):
        selection=manifest();rows,anchor=records(selection)
        candidate=next(r for r in rows if r['design_key']!=anchor)
        candidate['geometry']['guards'][0]['h']=-5e-16
        self.assertTrue(summarize(selection,rows)['comparisons'][candidate['job_id']]['target_check']['targets']['T50'])
        candidate['geometry']['guards'][0]['h']=-2e-12
        self.assertFalse(summarize(selection,rows)['comparisons'][candidate['job_id']]['target_check']['feasible'])
        candidate['geometry']['guards'][0]['h']=.1;candidate['mesh_pass']=False
        self.assertFalse(summarize(selection,rows)['comparisons'][candidate['job_id']]['target_check']['feasible'])
        candidate['mesh_pass']=True
        for c in candidate['currents']:
            c['torques']=[v-.02 for v in c['torques']];c['mean']-=.02
        self.assertFalse(summarize(selection,rows)['comparisons'][candidate['job_id']]['target_check']['feasible'])

    def test_target_survival_uses_theta0_at_the_same_refinement_settings(self):
        selection=manifest();rows,anchor=records(selection)
        selected=None
        for r in rows:
            if r['actual_spec']['name']!='L3':continue
            amp=.01 if r['design_key']==anchor else .0015
            for c in r['currents']:
                c['torques']=[c['mean']+amp*(-1 if i%2 else 1) for i in range(len(c['torques']))]
                c['std']=amp
            r['sum_variances']=3*amp*amp
            if r['design_key']!=anchor:selected=r['job_id']
        decision=summarize(selection,rows)['comparisons'][selected]['target_check']
        self.assertTrue(decision['targets']['T90'])

    def test_order_changes_are_reported_without_reselecting_designs(self):
        from selection import Design,Validation,select
        settings={'material':'fixture','current':1.0}
        theta=Design.from_values({'x':0.},settings)
        candidates=[Design.from_values({'x':v},settings) for v in (1.,2.)]
        source=[Validation('run'+str(i),'a'*64,method,'a',0,1,0,design,
                           10.,120.,.001,1.01,True,False,True,True,True,True)
                for i,(method,design) in enumerate(zip(('A','B'),candidates))]
        selection=select(source,f0=1.,mean0=1.,theta0=theta)
        rows,anchor=records(selection)
        for r in rows:
            if r['design_key']==anchor:continue
            first=r['design_key']==candidates[0].key
            reverse=r['actual_spec']['name']=='L3'
            amp=.0001 if first!=reverse else .0002
            for c in r['currents']:
                c['torques']=[c['mean']+amp*(-1 if i%2 else 1) for i in range(len(c['torques']))]
                c['std']=amp
            r['sum_variances']=len(r['currents'])*amp*amp
        report=summarize(selection,rows)
        primary=[r['design_key'] for r in report['feasible_orderings']['primary']]
        refined=[r['design_key'] for r in report['feasible_orderings']['L3']]
        self.assertEqual(primary,[d.key for d in candidates])
        self.assertEqual(refined,list(reversed(primary)))
        self.assertEqual(report['cohort'],selection['jobs'])


if __name__=='__main__':unittest.main(verbosity=2)
