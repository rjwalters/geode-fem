"""Coordinate/schema tests using committed definitions; no FEM is performed."""
import copy
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from design_evidence import RegisteredDesigns
from selection import Validation, select
from check_plan import make_plan

ROOT = HERE.parents[2]
REFERENCE = ROOT / 'benchmarks/motor_reference/reference_data.json'
STARTS = ROOT / 'benchmarks/motor_optimization/harness/starts.json'
PROVENANCE = {'source_commit': 'a' * 40, 'binary_sha256': 'b' * 64, 'source_files_sha256': 'c' * 64}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def definitions(reference=REFERENCE, starts=STARTS):
    return RegisteredDesigns.load(reference, starts, reference_sha256=digest(reference), starts_sha256=digest(starts))


def fixture(space='a'):
    starts = json.loads(STARTS.read_text())
    names = ['MAG','MH','MW','SW1','SW2','SW3','SW4','SD1','Sr1']
    if space == 'b':
        names += ['CPR1','CPR2','CPR3','CPR4','CPR5','CPR6','CPR7','CPS1','CPS2','CPS3']
    names += ['LENGTH','OPERATING_ANGLE','ScaleR']
    ready = {'ok': True, 'op': 'ready', 'protocol': 'icem-harness/1', 'space': starts['spaces'][space],
             'point_names': names, 'validation_msh_sha256': 'd' * 64,
             'session': {'validation': {'name':'primary','mode':'h93','torque':'h93/coupling','level':2,
                 'angles_deg':[float(i) for i in range(20)],'fractions':[0.,.5,1.],
                 'i_peak0_A':9.677657374172274,'mesh_repair':True,'newton_rel_tol':1e-8,
                 'newton_max_iterations':60,'policy_min_angle_deg':5.,'start':'cold'}}}
    return {'space': space, 'evaluator_ready': ready, 'anchor': {'msh_sha256': 'd' * 64}}, {'x': starts['starts']['theta0'][space]}


class DesignEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.definitions = definitions()

    def adapt(self, trace, record, provenance=PROVENANCE):
        try:
            return self.definitions.adapt(trace, record, provenance=provenance)
        except Exception as error:
            self.fail(f'valid coordinate evidence refused: {error}')

    def test_all_twenty_two_parameters_and_native_point_order_are_retained(self):
        trace, record = fixture('b')
        record['x'][8] = -.0004
        record['x'][15] = .0001
        out = self.adapt(trace, record)
        expected = {'MAG':.007,'MH':.007,'MW':.019,'SW1':.004,'SW2':.0023,'SW3':.001,
                    'SW4':.00825,'SD1':.135,'Sr1':.001,'LENGTH':.1,'OPERATING_ANGLE':0.,'ScaleR':1.,
                    'CPR1':-.0004,'CPR2':0.,'CPR3':0.,'CPR4':0.,'CPR5':0.,'CPR6':0.,'CPR7':0.,
                    'CPS1':.0001,'CPS2':0.,'CPS3':0.}
        self.assertEqual(out['parameters'], expected)
        values = [struct.unpack('>d', struct.pack('>Q', value))[0] for value in out['native_point_bits']]
        self.assertEqual(values, [expected[name] for name in trace['evaluator_ready']['point_names']])
        self.assertFalse(out['stator_offsets_zero'])
        self.assertFalse(out['can_launch'])

    def test_motion_maps_do_not_alias_but_training_method_and_resolution_do(self):
        a, ar = fixture('a'); b, br = fixture('b')
        da, db = self.adapt(a, ar), self.adapt(b, br)
        self.assertEqual(da['parameters'], db['parameters'])
        self.assertNotEqual(da['design'].key, db['design'].key)
        self.assertNotEqual(da['theta0'].key, db['theta0'].key)
        a.update(method='slsqp_l0', training_model={'train_level':0})
        a['evaluator_ready']['session']['training']={'level':0}
        self.assertEqual(da['design'].key, self.adapt(a, ar)['design'].key)

    def test_changed_source_binary_or_validator_mesh_changes_identity(self):
        trace, record = fixture();original=self.adapt(trace,record)
        for field in PROVENANCE:
            changed=dict(PROVENANCE);changed[field]='e'*(40 if field=='source_commit' else 64)
            self.assertNotEqual(original['design'].key,self.adapt(trace,record,changed)['design'].key)
        trace['evaluator_ready']['validation_msh_sha256']='f'*64
        trace['anchor']['msh_sha256']='f'*64
        self.assertNotEqual(original['design'].key,self.adapt(trace,record)['design'].key)

    def test_one_ulp_coordinates_and_signed_zero_are_preserved(self):
        trace,record=fixture('b');first=self.adapt(trace,record)
        record['x'][0]=math.nextafter(record['x'][0],math.inf)
        second=self.adapt(trace,record)
        self.assertNotEqual(first['design'].key,second['design'].key)
        self.assertEqual(second['parameters']['MAG'],record['x'][0])
        record['x'][15]=-0.0
        third=self.adapt(trace,record)
        self.assertNotEqual(second['design'].key,third['design'].key)
        self.assertTrue(third['stator_offsets_zero'])
        self.assertEqual(third['x_bits'][15],1<<63)

    def test_bad_coordinates_are_refused_without_clamping(self):
        trace,record=fixture()
        for change in ('short','long','bool','nan','below','above','bits'):
            bad=copy.deepcopy(record)
            if change=='short':bad['x'].pop()
            elif change=='long':bad['x'].append(0.)
            elif change=='bool':bad['x'][0]=True
            elif change=='nan':bad['x'][0]=float('nan')
            elif change=='below':bad['x'][0]=math.nextafter(.006,-math.inf)
            elif change=='above':bad['x'][0]=math.nextafter(.015,math.inf)
            else:bad['x_bits']=[0]*8
            with self.subTest(change=change),self.assertRaises(ValueError):
                self.definitions.adapt(trace,bad,provenance=PROVENANCE)
        for endpoint in (.006,.015):
            record['x'][0]=endpoint
            self.assertEqual(self.adapt(trace,record)['parameters']['MAG'],endpoint)

    def test_native_parameter_map_and_order_cannot_drift(self):
        for change in ('order','frozen','bound','map','point_names','protocol','status'):
            trace,record=fixture('b');ready=trace['evaluator_ready']
            if change=='order':ready['space']['free'].reverse()
            elif change=='frozen':ready['space']['frozen'][0]['value']=.11
            elif change=='bound':ready['space']['lower'][0]=.005
            elif change=='map':ready['space']['map']='scalar'
            elif change=='point_names':ready['point_names'].reverse()
            elif change=='protocol':ready['protocol']='unsupported'
            else:ready['ok']=False
            with self.subTest(change=change),self.assertRaisesRegex(ValueError,'coordinate map'):
                self.definitions.adapt(trace,record,provenance=PROVENANCE)

    def test_common_validator_settings_cannot_drift(self):
        changes={'level':1,'mode':'h45','fractions':[0.,1.],'angles_deg':[.5+i for i in range(20)],
                 'i_peak0_A':9.6777,'newton_rel_tol':1e-12,'newton_max_iterations':61,
                 'policy_min_angle_deg':4.,'mesh_repair':False,'start':'warm','torque':'arkkio'}
        for field,value in changes.items():
            trace,record=fixture();trace['evaluator_ready']['session']['validation'][field]=value
            with self.subTest(field=field),self.assertRaisesRegex(ValueError,'common primary model'):
                self.definitions.adapt(trace,record,provenance=PROVENANCE)
        trace,record=fixture();trace['anchor']['msh_sha256']='0'*64
        with self.assertRaisesRegex(ValueError,'meshes differ'):
            self.definitions.adapt(trace,record,provenance=PROVENANCE)

    def test_bound_definition_files_and_registered_mapping_are_checked(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);reference=root/'reference.json';starts=root/'starts.json'
            reference.write_bytes(REFERENCE.read_bytes());starts.write_bytes(STARTS.read_bytes())
            expected=digest(starts);starts.write_text(starts.read_text()+' ')
            with self.assertRaisesRegex(ValueError,'changed design-definition'):
                RegisteredDesigns.load(reference,starts,reference_sha256=digest(reference),starts_sha256=expected)
            for change in ('unknown_scalar','frozen','duplicate_bounds','space','start'):
                ref=json.loads(REFERENCE.read_text());st=json.loads(STARTS.read_text())
                if change=='unknown_scalar':ref['designs']['initial']['parameters_by_source_registration_order'].pop('MW')
                elif change=='frozen':ref['designs']['initial']['parameters_by_source_registration_order']['LENGTH']=.11
                elif change=='duplicate_bounds':ref['optimization_parameter_bounds'].append(ref['optimization_parameter_bounds'][0])
                elif change=='space':st['spaces']['b']['x0'][8]=-.001
                else:st['starts']['theta0']['a'][0]=.008
                reference.write_text(json.dumps(ref));starts.write_text(json.dumps(st))
                with self.subTest(change=change),self.assertRaises(ValueError):definitions(reference,starts)

    def test_input_and_output_settings_do_not_alias(self):
        trace,record=fixture();before=copy.deepcopy((trace,record));out=self.adapt(trace,record)
        out['settings']['common_validator']['angles_deg'].clear()
        out['parameters']['MAG']=.01
        self.assertEqual((trace,record),before)
        self.assertEqual(out['design'].key,self.adapt(trace,record)['design'].key)

    def test_missing_provenance_is_not_silently_filled(self):
        trace,record=fixture()
        for bad in ({},dict(PROVENANCE,binary_sha256='invalid'),dict(PROVENANCE,source_commit='x'*40)):
            with self.assertRaises(ValueError):self.definitions.adapt(trace,record,provenance=bad)

    def test_actual_parameter_maps_feed_matching_reference_schedule(self):
        values=[];initials=[]
        for space in ('a','b'):
            trace,record=fixture(space);record['x'][0]=.008
            out=self.adapt(trace,record);initials.append(out['theta0'])
            values.append(Validation(space,'a'*64,'A',space,0,1,0,out['design'],10.,120.,.001,1.01,
                eligible_run=True,host_contaminated=False,completed=True,geometry_admissible=True,
                discrete_pass=True,mesh_pass=True))
        selected=select(values,f0=1.,mean0=1.,theta0=initials[0],theta0_references=[initials[1]])
        plan=make_plan(selected)
        self.assertEqual(len(plan['reference_design_keys']),2)
        self.assertEqual(len(plan['jobs']),24)
        self.assertEqual(len(selected['records']),2)
        self.assertEqual(len({r.design.key for r in values}),2)


if __name__ == '__main__':
    unittest.main(verbosity=2)
