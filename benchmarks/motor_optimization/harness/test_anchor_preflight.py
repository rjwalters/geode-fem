"""Cross-platform tolerance and exact same-host dispatch checks; no FEM."""
import copy
import json
import math
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import anchor_preflight as check
import session
from test_session_journal import good_cal, good_anchor


def fixture():
    value = {'F': 1., 'full_current_mean': 2., 'msh_sha256': 'a'*64,
             'point_names': ['x'], 'point': [1.],
             'config': {'angles_deg': [0., 1.], 'level': 2},
             'currents': [{'fraction': fraction, 'i_peak_A': fraction*10.,
                           'torques': [.01, .02], 'newton_iterations': 5}
                          for fraction in (0., .5, 1.)]}
    frozen = {'anchor': {'F0': 1., 'mean0_full_current': 2., 'msh_sha256': 'a'*64},
              'validation_space_a': copy.deepcopy(value), 'validation_space_b': copy.deepcopy(value)}
    return value, frozen


class AnchorPreflightTests(unittest.TestCase):
    def test_observed_platform_noise_passes_without_changing_anchor_or_requiring_newton_counts(self):
        value, frozen = fixture()
        original = copy.deepcopy(frozen)
        value['F'] *= 1 + 2.4e-13
        value['full_current_mean'] *= 1 + 7.4e-15
        for current in value['currents']:
            current['torques'][0] += 1.1e-13
            current['newton_iterations'] += 1
        value['config']['mesh_repair'] = True
        result = check.compare(value, frozen, 'a')
        self.assertEqual(result['status'], 'PASS')
        self.assertEqual(frozen, original)
        self.assertNotEqual(result['currents'][0]['newton_iterations'],
                            result['currents'][0]['canonical_newton_iterations'])

    def test_each_relative_observable_refuses_two_e_minus_nine(self):
        for field in ('F', 'full_current_mean'):
            with self.subTest(field=field):
                value, frozen = fixture(); value[field] *= 1 + 2e-9
                self.assertEqual(check.compare(value, frozen, 'a')['status'], 'failed')

    def test_every_current_and_angle_refuses_two_e_minus_nine_newton_metres(self):
        for current in range(3):
            for angle in range(2):
                with self.subTest(current=current, angle=angle):
                    value, frozen = fixture(); value['currents'][current]['torques'][angle] += 2e-9
                    self.assertEqual(check.compare(value, frozen, 'a')['status'], 'failed')

    def test_missing_nonfinite_or_mismatched_samples_do_not_pass(self):
        changes = [lambda v: v.update(F=float('nan')),
                   lambda v: v['currents'][1]['torques'].__setitem__(0, float('inf')),
                   lambda v: v['currents'][1]['torques'].pop(),
                   lambda v: v['currents'].pop(),
                   lambda v: v.update(msh_sha256='b'*64),
                   lambda v: v['config'].update(angles_deg=[.5, 1.5]),
                   lambda v: v.update(point=[1.01]),
                   lambda v: v['currents'][1].update(fraction=.25)]
        for index, mutate in enumerate(changes):
            with self.subTest(index=index):
                value, frozen = fixture(); mutate(value)
                with self.assertRaises(ValueError): check.compare(value, frozen, 'a')

    def test_same_host_signature_distinguishes_signed_zero(self):
        a, _ = fixture(); b = copy.deepcopy(a)
        a['currents'][0]['torques'][0] = 0.
        b['currents'][0]['torques'][0] = -0.
        self.assertNotEqual(check.signature(a), check.signature(b))

    def test_calibration_requires_recorded_preflight_and_exact_values_from_every_process(self):
        self.assertTrue(check.calibration_matches(good_cal(), good_anchor()))
        for place in ('aggregate', 'single', 'worker'):
            with self.subTest(place=place):
                cal = good_cal()
                value = (cal['theta0_validation'] if place == 'aggregate' else
                         cal['single_process']['theta0_validation'] if place == 'single' else
                         cal['per_process'][0]['theta0_validation'])
                value['F'] = math.nextafter(value['F'], math.inf)
                self.assertFalse(check.calibration_matches(cal, good_anchor()))
        for report in (None, {}, dict(good_anchor(), status='failed'),
                       dict(good_anchor(), spaces_agree_bitwise=False)):
            self.assertFalse(check.calibration_matches(good_cal(), report))
        cal = good_cal(); cal['per_process'] = []
        self.assertFalse(check.calibration_matches(cal, good_anchor()))

    def test_four_ulp_recomputation_flag_is_retained_but_not_the_cross_platform_gate(self):
        cal = good_cal(); cal['anchor_agrees_4ulp'] = False
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            try:
                session.accept_calibration(cal, root, 'accepted.json', True, anchor_check=good_anchor())
            except SystemExit as error:
                self.fail(f'valid cross-platform comparison was rejected: {error}')
            saved = json.loads((root/'accepted.json').read_text())
            self.assertTrue(saved['accepted_for_dispatch'])
            self.assertFalse(saved['calibration']['anchor_agrees_4ulp'])
            with self.assertRaisesRegex(SystemExit, 'calibration rejected'):
                session.accept_calibration(cal, root, 'missing.json', True)

    def test_failed_anchor_preflight_never_calibrates_or_dispatches(self):
        with tempfile.TemporaryDirectory() as td:
            binary = Path(td)/'eval'; binary.write_bytes(b'fixture')
            with patch.object(session, 'host_record', return_value={}), \
                 patch.object(session, 'provenance', return_value={}), \
                 patch('build_provenance.verify', return_value={'verified': True}), \
                 patch.object(session, 'sha256', return_value='fixture'), \
                 patch('start_preflight.verify', return_value={'status': 'PASS'}), \
                 patch('anchor_preflight.verify', return_value={'status': 'failed'}), \
                 patch.object(session, 'gated_calibration', side_effect=AssertionError('calibration must not begin')) as calibrate, \
                 patch.object(session, 'schedule') as dispatch:
                with self.assertRaisesRegex(SystemExit, 'anchor recomputation failed'):
                    session.main(['--main', '--procs', '1', '--out', str(Path(td)/'session'),
                                  '--binary', str(binary)])
                calibrate.assert_not_called(); dispatch.assert_not_called()


if __name__ == '__main__':
    unittest.main()
