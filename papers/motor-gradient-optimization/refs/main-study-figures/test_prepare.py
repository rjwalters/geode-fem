"""Synthetic presentation checks: no optimizer runs or measured motor outcomes."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from prepare import METHODS, TARGETS, load, prepare


def fixture():
    """Deliberately simple 100-job fixture, including early failures and absent data."""
    attempts = []; records = []
    for method in METHODS:
        for space in ('a', 'b'):
            for start in range(5):
                for repeat in (1, 2):
                    rid = f'synthetic/{method}/{space}/{start}/{repeat}'
                    stop = 'error' if start == 2 else 'budget'
                    end = 30. if start == 2 else 120.
                    attempt = {'run_id': rid, 'job': {'method_letter': method, 'method': METHODS[method][0],
                        'space': space, 'start_index': start, 'start': 'theta0' if start == 0 else f'lhs{start}',
                        'repeat': repeat}, 'selected_completion': True, 'terminal_bound': True,
                        'host_contaminated': False, 'trace_sha256': 'a' * 64, 'status': stop,
                        'ledger': {'total_fe': end, 'total_seconds': end * 2., 'solve_fe': end}, 'validations': []}
                    attempts.append(attempt)
                    if start == 4:
                        continue
                    schedule = [(10., .4), (20., .08)] if start == 2 else [(10., .4), (60., .08), (80., .005)]
                    for ordinal, (fe, f) in enumerate(schedule):
                        if repeat == 2:
                            fe *= .9
                        r = {'run_id': rid, 'source_sha256': 'a' * 64, 'method': method, 'space': space,
                             'start_index': start, 'repeat': repeat, 'ordinal': ordinal,
                             'elapsed_seconds': fe * 2., 'budget_seconds': 240., 'f_val': f, 'mean_full': 1.1,
                             'eligible_run': True, 'host_contaminated': False, 'completed': True,
                             'geometry_admissible': True, 'discrete_pass': True, 'mesh_pass': True}
                        upper = f + max(.1 * f, 2.5e-7)
                        records.append({'record': r, 'eligible': True, 'rejection_reasons': [], 'f_upper': upper,
                                        'attains': {n: upper <= fraction for n, fraction in TARGETS.items()}})
                        attempt['validations'].append({'record_id': f'{rid}:{ordinal}'})
    return {'schema': 1, 'can_select': True, 'capture_manifest_sha256': 'b' * 64,
            'source_commit': 'c' * 40, 'attempts': attempts,
            'selection': {'f0': 1., 'mean0': 1., 'mean_floor': 1.0025,
                          'targets': dict(TARGETS), 'records': records}}


def preview_fixture():
    """Vary synthetic paths so the preview exercises all five visual encodings."""
    data = fixture()
    factors = {'A': (1., 1.), 'B': (1.25, 1.15), 'C': (3., 1.35),
               'D': (.8, .9), 'E': (.65, .8)}
    for audit in data['selection']['records']:
        r = audit['record']; quality, time = factors[r['method']]
        r['f_val'] *= quality; r['elapsed_seconds'] *= time
        upper = r['f_val'] + max(.1 * r['f_val'], 2.5e-7)
        audit['f_upper'] = upper
        audit['attains'] = {n: upper <= fraction for n, fraction in TARGETS.items()}
    return data


class PresentationTests(unittest.TestCase):
    def test_complete_table_and_exact_checkpoint(self):
        data = fixture(); original = copy.deepcopy(data); out = prepare(data, 2.)
        self.assertEqual(data, original)
        self.assertEqual(len(out['runs']), 100); self.assertEqual(len(out['attainment']), 300)
        first = out['runs'][0]
        self.assertAlmostEqual(first['checkpoint_upper_ratio']['60'], .088)
        self.assertAlmostEqual(first['checkpoint_upper_ratio']['120'], .0055)
        self.assertEqual(first['first_attainment']['T99']['fe'], 80.)
        self.assertEqual(out['runs'][1]['first_attainment']['T99']['fe'], 72.)

    def test_failed_run_keeps_earlier_crossing_without_extending_curve(self):
        row = next(r for r in prepare(fixture(), 2.)['runs'] if r['start_index'] == 2)
        self.assertEqual(row['stop'], 'error'); self.assertEqual(row['points'][-1][0], 30.)
        self.assertEqual(row['first_attainment']['T90']['fe'], 20.)
        self.assertNotIn('T99', row['first_attainment'])

    def test_no_validation_is_explicit_not_zero_or_fake_speedup(self):
        out = prepare(fixture(), 2.)
        absent = [r for r in out['runs'] if r['start_index'] == 4]
        self.assertEqual(len(absent), 20)
        self.assertTrue(all(r['points'] == [] and r['checkpoint_upper_ratio']['60'] is None for r in absent))
        cells = [r for r in out['attainment'] if r['start_index'] == 4]
        self.assertTrue(all(r['status'] == 'not_attained' and r['attainment_fe'] is None for r in cells))

    def test_rejected_validation_cannot_improve_curve(self):
        data = fixture(); row = data['selection']['records'][2]
        row['eligible'] = False; row['record']['geometry_admissible'] = False
        row['rejection_reasons'] = ['geometry_not_admissible']; row['attains'] = dict.fromkeys(TARGETS, False)
        out = prepare(data, 2.)
        self.assertNotIn('T99', out['runs'][0]['first_attainment'])
        self.assertAlmostEqual(out['runs'][0]['points'][-1][1], .088)

    def test_excluded_attempt_retained_separately(self):
        data = fixture(); extra = copy.deepcopy(data['attempts'][0])
        extra.update(run_id='synthetic/earlier-interruption', selected_completion=False,
                     terminal_bound=False, status='interrupted', validations=[])
        data['attempts'].append(extra)
        out = prepare(data, 2.)
        self.assertEqual(len(out['runs']), 100); self.assertEqual(out['excluded_attempts'], [extra])

    def test_incomplete_or_ambiguous_cohort_refused(self):
        for kind in ('missing', 'duplicate'):
            data = fixture()
            if kind == 'missing':
                data['attempts'].pop()
            else:
                extra = copy.deepcopy(data['attempts'][0]); extra['run_id'] += '/duplicate'
                data['attempts'].append(extra)
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                prepare(data, 2.)

    def test_semantic_mutations_refused(self):
        for kind in ('unit', 'attainment', 'margin', 'geometry', 'census', 'late', 'trace'):
            data = fixture(); row = data['selection']['records'][0]; unit = 2.
            if kind == 'unit': unit = 3.
            elif kind == 'attainment': row['attains']['T99'] = True
            elif kind == 'margin': row['f_upper'] = .01
            elif kind == 'geometry': row['record']['geometry_admissible'] = False
            elif kind == 'census': data['attempts'][0]['validations'].pop()
            elif kind == 'late': row['record']['elapsed_seconds'] = 241.
            elif kind == 'trace': row['record']['source_sha256'] = 'd' * 64
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                prepare(data, unit)

    def test_digest_binds_supplied_selection(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'selection.json'; raw = json.dumps(fixture()).encode(); path.write_bytes(raw)
            expected = hashlib.sha256(raw).hexdigest()
            self.assertEqual(load(path, expected, 2.)['selection_sha256'], expected)
            path.write_bytes(raw + b' ')
            with self.assertRaisesRegex(ValueError, 'digest'):
                load(path, expected, 2.)

    def test_varied_preview_is_explicit_synthetic_fixture(self):
        out = prepare(preview_fixture(), 2.)
        first = [r for r in out['runs'] if (r['space'], r['start_index'], r['repeat']) == ('a', 0, 1)]
        self.assertEqual(len({json.dumps(r['points']) for r in first}), 5)
        self.assertTrue(all(r['run_id'].startswith('synthetic/') for r in out['runs']))


if __name__ == '__main__':
    unittest.main()
