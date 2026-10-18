"""Recorded validation semantics with synthetic evidence; no FEM results."""
import copy
import math
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parent))
import validation_evidence as check
from selection import Design, select

ANCHOR={'F0':1.,'mean0_full_current':1.,'msh_sha256':'a'*64}
SPECS={'clearance':{'scale':2.,'unit':'m','strict':True}}
ARGS={'anchor':ANCHOR,'unit_seconds':2.,'budget_fe':120.,'guard_specs':SPECS,
      'discrete_names':('components_finite',)}


def quality():
    return {'inverted':0,'min_angle_deg':10.,'max_angle_deg':130.,'min_area_ratio':1.,'max_area_ratio':1.}


def fixture(*,f=.04,mean=1.01,c=1.,seconds=40.):
    h=c-.05;buildable=c>0;exact=buildable and h>=0;credit=buildable and h>=-1e-12
    delta=max(.1*f,2.5e-7);mean_ok=mean>=1.0025
    return {'index':0,'status':'ok','seconds':seconds,'wall_fe':seconds/2.,'credited':seconds<=240.,
        'F_val':f,'mean_full':mean,'fractions':[0.,.5,1.],'means':[0.,.4,mean],
        'stds':[math.sqrt(f),0.,0.],'delta_F':delta,'F_upper':f+delta,'mean_floor':1.0025,
        'mean_ok':mean_ok,'admissible':credit,'h_nonnegative':exact,'min_h':h,
        'attained':{k:f+delta<=t and mean_ok and credit for k,t in check.TARGETS.items()},
        'mesh':{'kind':'moved','quality':quality()},'msh_sha256':'a'*64,
        'geometry':{'buildable':buildable,'admissible':exact,'credit_admissible':credit,'construction_error':None,
                    'guards':[{'name':'clearance','unit':'m','margin':2*c,'scale':2.,'c':c,'h':h,'pass':buildable}],
                    'discrete':[{'name':'components_finite','pass':True}]}}


def attempt(index,passed=False):
    return {'attempt':index,'label':check.REPAIR_LABELS[index],
            'outcome':'passed' if passed else 'mesher_failed','detail':None if passed else 'fixture failure',
            'msh_sha256':'b'*64 if passed else None,'mesh_s':.1,'setup_s':.2}


def fresh():
    r=fixture();moved=quality();moved['min_angle_deg']=4.
    r.update(msh_sha256='b'*64,mesh={'kind':'fresh','quality':quality(),'msh_sha256':'b'*64,
        'moved_quality':moved,'trigger':{'kind':'min_angle','min_angle_deg':4.,'threshold_deg':5.},
        'attempts':[attempt(0),attempt(1,True)]})
    return r


def infeasible():
    r=fixture();r.update(status='mesh_infeasible',F_val=None,mean_full=None,admissible=False,mean_ok=False,
        attained={name:False for name in check.TARGETS},attempts=[attempt(i) for i in range(4)])
    for k in ('mesh','msh_sha256','h_nonnegative','min_h','stds','means','fractions','delta_F','F_upper','mean_floor'):
        r.pop(k)
    return r


class ValidationEvidenceTests(unittest.TestCase):
    def verified(self, record):
        try:
            return check.verify_validation(record,**ARGS)
        except ValueError as error:
            self.fail(f'valid recorded evidence was rejected: {error}')

    def test_completed_primary_evidence_preserves_input_and_target_credit(self):
        r=fixture();before=copy.deepcopy(r);out=check.verify_validation(r,**ARGS)
        self.assertEqual(r,before);self.assertTrue(out['completed']);self.assertTrue(out['mesh_pass'])
        self.assertEqual(out['target_credit'],{'T50':True,'T90':True,'T99':False})

    def test_post_budget_target_is_retained_without_credit(self):
        r=fixture(seconds=241.);self.assertTrue(r['attained']['T50'])
        out=check.verify_validation(r,**ARGS)
        self.assertFalse(out['within_budget']);self.assertFalse(any(out['target_credit'].values()))
        self.assertTrue(r['attained']['T50'])

    def test_exact_budget_boundary_and_first_later_float(self):
        for seconds,expected in ((240.,True),(math.nextafter(240.,math.inf),False)):
            with self.subTest(seconds=seconds):
                self.assertEqual(self.verified(fixture(seconds=seconds))['within_budget'],expected)

    def test_changed_time_unit_or_credit_flag_is_refused(self):
        for change in ({'seconds':41.},{'wall_fe':21.},{'credited':False}):
            with self.subTest(change=change),self.assertRaises(ValueError):
                check.verify_validation(dict(fixture(),**change),**ARGS)
        with self.assertRaises(ValueError):check.verify_validation(fixture(),**{**ARGS,'unit_seconds':3.})

    def test_mean_floor_and_exact_target_boundaries(self):
        for mean in (1.,math.nextafter(1.0025,-math.inf),1.0025):
            out=self.verified(fixture(mean=mean))
            self.assertEqual(out['target_credit']['T50'],mean>=1.0025)
        r=fixture(f=1e-8);self.assertEqual(r['delta_F'],2.5e-7)
        self.assertTrue(check.verify_validation(r,**ARGS)['target_credit']['T99'])

    def test_tiny_negative_h_retains_credit_but_larger_violation_does_not(self):
        for c,expected in ((.05-5e-16,True),(.05-2e-12,False)):
            r=fixture(c=c);self.assertFalse(r['h_nonnegative'])
            out=self.verified(r)
            self.assertEqual(out['geometry_admissible'],expected)
            self.assertEqual(out['target_credit']['T50'],expected)

    def test_guard_normalization_scale_unit_and_strictness_are_checked(self):
        for change in ({'scale':3.},{'unit':'mm'},{'margin':1.},{'c':.5},{'h':.5},{'pass':False}):
            r=fixture();r['geometry']['guards'][0].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):check.verify_validation(r,**ARGS)
        r=fixture(c=0.)
        self.assertFalse(check.verify_validation(r,**ARGS)['buildable'])
        with self.assertRaises(ValueError):
            check.verify_validation(r,**{**ARGS,'guard_specs':{'clearance':{**SPECS['clearance'],'strict':False}}})

    def test_empty_or_missing_registered_guards_are_refused(self):
        for kind in ('guards','discrete'):
            r=fixture();r['geometry'][kind]=[]
            with self.assertRaises(ValueError):check.verify_validation(r,**ARGS)

    def test_changed_current_set_or_inconsistent_aggregates_are_refused(self):
        for change in ({'fractions':[0.,.25,1.]},{'stds':[.3,0.,0.]},{'means':[0.,.4,2.]},
                       {'F_val':.05},{'mean_full':2.},{'stds':[-.2,0.,0.]},{'means':[]},
                       {'stds':[1e308,0.,0.]}):
            with self.subTest(change=change),self.assertRaises(ValueError):
                check.verify_validation(dict(fixture(),**change),**ARGS)

    def test_arithmetic_and_target_flags_cannot_disagree(self):
        for change in ({'delta_F':.1},{'F_upper':.5},{'mean_floor':1.},
                       {'mean_ok':False},{'attained':{'T50':True}},{'attained':dict.fromkeys(check.TARGETS,True)}):
            with self.subTest(change=change),self.assertRaises(ValueError):
                check.verify_validation(dict(fixture(),**change),**ARGS)

    def test_mesh_quality_threshold_is_inclusive_and_inversion_refuses(self):
        r=fixture();r['mesh']['quality']['min_angle_deg']=5.
        self.assertTrue(check.verify_validation(r,**ARGS)['mesh_pass'])
        for change in ({'min_angle_deg':math.nextafter(5.,0.)},{'inverted':1},
                       {'min_area_ratio':0.},{'min_angle_deg':None},{'inverted':False}):
            r=fixture();r['mesh']['quality'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):check.verify_validation(r,**ARGS)

    def test_unknown_kind_and_wrong_reference_mesh_are_refused(self):
        r=fixture();r['mesh']['kind']='historical'
        with self.assertRaises(ValueError):check.verify_validation(r,**ARGS)
        r=fixture();r['msh_sha256']='b'*64
        with self.assertRaises(ValueError):check.verify_validation(r,**ARGS)

    def test_fresh_mesh_requires_final_passing_attempt_and_matching_hashes(self):
        r=fresh();self.assertTrue(check.verify_validation(r,**ARGS)['mesh_pass'])
        for mutate in (lambda r:r['mesh'].update(msh_sha256='c'*64),
                       lambda r:r['mesh']['attempts'][-1].update(msh_sha256='c'*64),
                       lambda r:r['mesh'].update(attempts=[]),
                       lambda r:r['mesh']['attempts'][0].update(outcome='passed'),
                       lambda r:r['mesh']['attempts'][-1].update(outcome='mesher_failed')):
            r=fresh();mutate(r)
            with self.assertRaises(ValueError):check.verify_validation(r,**ARGS)

    def test_fresh_mesh_trigger_must_match_failed_moved_mesh(self):
        for mutate in (lambda r:r['mesh'].update(trigger=None),
                       lambda r:r['mesh']['moved_quality'].update(min_angle_deg=6.),
                       lambda r:r['mesh']['trigger'].update(threshold_deg=6.),
                       lambda r:r['mesh']['trigger'].update(min_angle_deg=3.)):
            r=fresh();mutate(r)
            with self.assertRaises(ValueError):check.verify_validation(r,**ARGS)
        r=fresh();r['mesh']['moved_quality'].update(inverted=3,min_area_ratio=-1.,min_angle_deg=None)
        r['mesh']['trigger']={'kind':'inverted','triangles':3,'min_area_ratio':-1.}
        self.assertTrue(check.verify_validation(r,**ARGS)['mesh_pass'])

    def test_four_failed_repairs_remain_explicitly_ineligible(self):
        r=infeasible();before=copy.deepcopy(r);out=check.verify_validation(r,**ARGS)
        self.assertEqual(r,before);self.assertFalse(out['mesh_pass']);self.assertFalse(out['completed'])
        self.assertTrue(out['geometry_admissible']);self.assertFalse(any(out['target_credit'].values()))
        self.assertTrue(out['within_budget'])

    def test_infeasible_mesh_cannot_gain_values_or_drop_attempts(self):
        for mutate in (lambda r:r.update(F_val=.01),lambda r:r['attempts'].pop(),
                       lambda r:r['attempts'][0].update(outcome='passed'),
                       lambda r:r['attempts'][1].update(label='default')):
            r=infeasible();mutate(r)
            with self.assertRaises(ValueError):check.verify_validation(r,**ARGS)

    def test_repair_failure_reason_and_attempt_costs_are_checked(self):
        for change in ({'mesh_s':-1.},{'setup_s':math.nan},{'outcome':'fatal'},
                       {'detail':None},{'attempt':True}):
            r=fresh();r['mesh']['attempts'][0].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):check.verify_validation(r,**ARGS)
        r=fresh();a=r['mesh']['attempts'][0]
        a.update(outcome='policy_failed',msh_sha256='d'*64,
                 detail={'kind':'min_angle','min_angle_deg':4.,'threshold_deg':5.})
        self.assertTrue(check.verify_validation(r,**ARGS)['mesh_pass'])
        a['detail']['min_angle_deg']=6.
        with self.assertRaises(ValueError):check.verify_validation(r,**ARGS)

    def test_nonfinite_quantities_and_nonboolean_verdicts_fail_closed(self):
        for key in ('F_val','mean_full','seconds','wall_fe'):
            for value in (None,True,math.nan,math.inf):
                with self.subTest(key=key,value=value),self.assertRaises(ValueError):
                    check.verify_validation(dict(fixture(),**{key:value}),**ARGS)
        for key in ('credited','mean_ok','admissible'):
            with self.assertRaises(ValueError):check.verify_validation(dict(fixture(),**{key:1}),**ARGS)

    def test_adapter_preserves_earlier_validations_of_failed_runs_in_selection(self):
        context={'run_id':'original#7','source_sha256':'f'*64,'method':'A','space':'a',
                 'start_index':0,'repeat':1,'eligible_run':True,'host_contaminated':False,'stop_reason':'error'}
        design=Design.from_values({'fixture_x':1.},{'fixture_physics':'synthetic'})
        raw=fixture(c=.05-5e-16)
        adapted=check.adapt_validation(raw,context=context,design=design,**ARGS)
        selection=select([adapted],f0=1.,mean0=1.,theta0=design)
        self.assertEqual(selection['cells'][0]['record_id'],'original#7:0')
        self.assertEqual(selection['records'][0]['record']['stop_reason'],'error')
        self.assertTrue(selection['records'][0]['attains']['T50'])
        self.assertFalse(raw['h_nonnegative'])

    def test_adapter_retains_every_ineligible_observation_with_selection_reasons(self):
        base={'run_id':'original#7','source_sha256':'f'*64,'method':'A','space':'a',
              'start_index':0,'repeat':1,'eligible_run':True,'host_contaminated':False,'stop_reason':'error'}
        design=Design.from_values({'fixture_x':1.},{'fixture_physics':'synthetic'})
        cases=((fixture(seconds=241.),{},'validation_not_within_budget'),
               (infeasible(),{},'mesh_policy_not_passed'),
               (fixture(),{'eligible_run':False},'run_not_eligible'),
               (fixture(),{'host_contaminated':True},'host_contaminated_or_unknown'),
               (fixture(),{'host_contaminated':None},'host_contaminated_or_unknown'))
        for raw,change,reason in cases:
            with self.subTest(reason=reason,change=change):
                adapted=check.adapt_validation(raw,context={**base,**change},design=design,**ARGS)
                out=select([adapted],f0=1.,mean0=1.,theta0=design)
                self.assertEqual(len(out['records']),1)
                self.assertIn(reason,out['records'][0]['rejection_reasons'])
                self.assertEqual(out['cells'][0]['status'],'unavailable')


if __name__=='__main__':unittest.main(verbosity=2)
