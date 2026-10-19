"""Synthetic recorded-margin checks; no FEM or main-result evidence."""
import copy
from dataclasses import replace
import math
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from geometry_evidence import verify_geometry
from test_selection import record, selection, cell

NAMES = {'guard_names': ('clearance', 'width'), 'discrete_names': ('components_finite',)}


def fixture(h=0.0, *, smooth=True, discrete=True, construction_error=None):
    buildable = smooth and discrete and construction_error is None
    exact = buildable and h >= 0
    return {'admissible': buildable and h >= -1e-12,
            'h_nonnegative': exact, 'min_h': min(h, 0.95),
            'geometry': {'buildable': buildable, 'admissible': exact,
                         'construction_error': construction_error,
                         'guards': [{'name': 'clearance', 'h': h, 'pass': smooth},
                                    {'name': 'width', 'h': 0.95, 'pass': True}],
                         'discrete': [{'name': 'components_finite', 'pass': discrete}]}}


class GeometryEvidenceTests(unittest.TestCase):
    def test_tiny_negative_margin_retains_representative_and_crossing(self):
        raw = fixture(-5e-16); before = copy.deepcopy(raw)
        verdict = verify_geometry(raw, **NAMES)
        self.assertFalse(raw['h_nonnegative'])
        self.assertTrue(verdict['geometry_admissible'])
        adapted = replace(record(), geometry_admissible=verdict['geometry_admissible'],
                          discrete_pass=verdict['discrete_pass'])
        out = selection([adapted])
        self.assertEqual(cell(out)['record_id'], adapted.record_id)
        self.assertEqual(cell(out, 'A/a/T50')['record_id'], adapted.record_id)
        self.assertEqual(raw, before)

    def test_closed_tolerance_boundary_and_next_float_below_it(self):
        for h, eligible in ((0.0, True), (-1e-12, True),
                            (math.nextafter(-1e-12, -math.inf), False), (-2e-12, False)):
            with self.subTest(h=h):
                verdict = verify_geometry(fixture(h), **NAMES)
                self.assertIs(verdict['geometry_admissible'], eligible)
                out = selection([replace(record(), geometry_admissible=eligible)])
                self.assertEqual(cell(out)['status'], 'selected' if eligible else 'unavailable')

    def test_construction_and_guard_failure_stay_ineligible(self):
        for args in ({'smooth': False}, {'discrete': False}, {'construction_error': 'failed'}):
            with self.subTest(args=args):
                verdict = verify_geometry(fixture(**args), **NAMES)
                self.assertFalse(verdict['geometry_admissible'])
                self.assertFalse(verdict['buildable'])

    def test_missing_extra_duplicate_and_empty_guard_sets_are_refused(self):
        for field in ('guards', 'discrete'):
            for mutation in ('empty', 'duplicate', 'rename'):
                raw = fixture()
                rows = raw['geometry'][field]
                if mutation == 'empty': rows.clear()
                elif mutation == 'duplicate': rows.append(copy.deepcopy(rows[0]))
                else: rows[0]['name'] = 'unregistered'
                with self.subTest(field=field, mutation=mutation), self.assertRaises(ValueError):
                    verify_geometry(raw, **NAMES)
        for key in NAMES:
            for names in ((), ('duplicate', 'duplicate'), (None,)):
                with self.subTest(key=key, names=names), self.assertRaises(ValueError):
                    verify_geometry(fixture(), **{**NAMES, key: names})

    def test_all_verdict_flags_must_be_boolean_and_consistent(self):
        for location, key in (('top', 'admissible'), ('top', 'h_nonnegative'),
                              ('geometry', 'buildable'), ('geometry', 'admissible')):
            for value in (False, 1, None):
                raw = fixture()
                target = raw if location == 'top' else raw['geometry']
                target[key] = value
                with self.subTest(location=location, key=key, value=value), self.assertRaises(ValueError):
                    verify_geometry(raw, **NAMES)
        for field in ('guards', 'discrete'):
            raw = fixture(); raw['geometry'][field][0]['pass'] = 1
            with self.assertRaises(ValueError): verify_geometry(raw, **NAMES)

    def test_missing_nonfinite_and_boolean_margins_are_refused(self):
        for value in (None, True, math.nan, math.inf, -math.inf):
            raw = fixture(); raw['geometry']['guards'][0]['h'] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                verify_geometry(raw, **NAMES)
        raw = fixture(); raw['min_h'] = 0.1
        with self.assertRaisesRegex(ValueError, 'min_h'): verify_geometry(raw, **NAMES)
        raw = fixture(); del raw['geometry']['construction_error']
        with self.assertRaisesRegex(ValueError, 'construction'): verify_geometry(raw, **NAMES)


if __name__ == '__main__':
    unittest.main(verbosity=2)
